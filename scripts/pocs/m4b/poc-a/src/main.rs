//! M4-B PoC A：最小 WebView2 surface 验证链
//! 验证项：① 独立 profile（user data folder 隔离）→ ② navigate https → ③ 页面文本快照 → ④ 截图（降级）→ ⑤ 自动关闭退出
//!
//! 已知 API 事实（wry 0.55.1 源码确认）：
//! - evaluate_script 返回 Result<()> 不返回结果 → 必须用 evaluate_script_with_callback，
//!   回调收到 WebView2 ExecuteScript 的 JSON 序列化值（字符串带引号，需 serde_json 解析）
//! - wry 0.55/0.56 均无 capture API → 截图验证降级为 title + innerText 长度 + url()
//! - 回调运行在 WebView2 线程，闭包需 Send + 'static；UI 状态变更走 EventLoopProxy

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{PageLoadEvent, WebContext, WebViewBuilder};

/// 主事件循环的自定义事件
enum UserEvent {
    /// 页面加载完成（Finished）→ 触发取快照
    SnapshotTaken,
    /// 快照已记录 → 退出进程
    Exit,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ① 独立 user data folder：WebContext::new(Some(dir)) 的 data_directory
    //    直通 CreateCoreWebView2EnvironmentWithOptions 的 userDataFolder 参数
    //    不同 data_directory = 完全隔离的 profile（cookies/storage/cache）
    let profile_dir = std::env::temp_dir().join("m4b-poc-a-profile");
    println!("[profile] {}", profile_dir.display());
    let mut context = WebContext::new(Some(profile_dir));

    // ② tao 事件循环 + 窗口（wry 不提供事件循环，宿主负责）
    // tao 0.35.3: generic user events go through EventLoopBuilder::<T>::with_user_event().build()
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let proxy_page = proxy.clone();
    let window = WindowBuilder::new()
        .with_title("M4B PoC A: human_surface")
        .with_inner_size(tao::dpi::LogicalSize::new(1024.0, 768.0))
        .build(&event_loop)?;

    // ③ WebView 构建
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        // 验证项 ②：navigate https URL
        .with_url("https://example.com")
        // 拦截点（M4-C 的 navigation deny 逻辑挂这里）：true = 放行，false = 拒绝
        .with_navigation_handler(|url| {
            println!("[nav] {url}");
            true
        })
        // 验证项 ③：页面加载完成后取文本快照
        .with_on_page_load_handler(move |ev, url| {
            if let PageLoadEvent::Finished = ev {
                println!("[load finished] {url}");
                let _ = proxy_page.send_event(UserEvent::SnapshotTaken);
            }
        })
        .build(&window)?;

    // 快照是否已取（30s 超时保护用）
    let snapshot_done = Rc::new(Cell::new(false));

    // ④ 事件循环：Finished → 取快照 → 自动退出；30s 无 Finished 则超时退出（exit 2）
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::NewEvents(StartCause::Init) => {
                *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(30));
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if !snapshot_done.get() {
                    println!("[timeout] page did not finish loading within 30s");
                    *control_flow = ControlFlow::ExitWithCode(2);
                } else {
                    *control_flow = ControlFlow::Wait;
                }
            }
            Event::UserEvent(UserEvent::SnapshotTaken) => {
                snapshot_done.set(true);
                let proxy_exit = proxy.clone();
                let _ = webview.evaluate_script_with_callback(
                    "JSON.stringify({t: document.title, n: document.body ? document.body.innerText.length : -1, s: document.body ? document.body.innerText.slice(0, 200) : ''})",
                    move |res| {
                        println!("[snapshot raw] {res}");
                        // WebView2 ExecuteScript 回调收到的是 JSON 序列化字符串（外层带引号），
                        // 需先解外层 String，再解析内层 JSON 对象
                        if let Ok(outer) = serde_json::from_str::<serde_json::Value>(&res) {
                            if let Some(inner_str) = outer.as_str() {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(inner_str) {
                                    println!(
                                        "[snapshot] title={} innerText_len={}",
                                        v["t"].as_str().unwrap_or("?"),
                                        v["n"].as_i64().unwrap_or(-1)
                                    );
                                }
                            }
                        }
                        let _ = proxy_exit.send_event(UserEvent::Exit);
                    },
                );
            }
            Event::UserEvent(UserEvent::Exit) => {
                println!(r"[exit] done. profile kept at %TEMP%\m4b-poc-a-profile");
                *control_flow = ControlFlow::ExitWithCode(0);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => *control_flow = ControlFlow::Wait,
        }
    });
}
