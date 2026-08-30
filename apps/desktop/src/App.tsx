import { I18nProvider } from "./i18n";
import { ShellApp } from "../features/shell-ui/src/ShellApp";

export function App() {
  return (
    <I18nProvider>
      <ShellApp />
    </I18nProvider>
  );
}
