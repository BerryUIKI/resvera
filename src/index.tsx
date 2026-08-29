import { render } from "solid-js/web";
import { App } from "./App";
import { I18nProvider } from "./i18n";
import "./index.css";

const root = document.getElementById("root");

if (root) {
  render(
    () => (
      <I18nProvider defaultLocale="zh-CN">
        <App />
      </I18nProvider>
    ),
    root
  );
}
