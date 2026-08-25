# PTY and Browser Security

## Terminal

Human Terminal Surface 与 Agent Terminal Automation 是不同 capability。Agent 不得默认接管用户现有 PTY。Create 必须验证 cwd 与 workspace scope；write/resize/close 使用 opaque resource ID；close/disconnect/revoke 必须清理 lease。

## Browser

Browser provider 使用独立 profile/session。Snapshot、navigate、interact、download、credential/autofill、clipboard 和 file chooser 分权。Agent 只得到经 policy 的 action，不得到 raw CDP。

## Shared Resource

Human takeover 会暂停或撤销 Agent mutation lease。页面显示当前 owner、Agent action 和敏感上下文提示。Browser/PTY audit 不记录密码或完整终端秘密输出。
