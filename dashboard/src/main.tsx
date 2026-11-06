import { render } from "solid-js/web";
import { App } from "./app/App";
import "./shared/styles/tokens.css";
import "./shared/styles/base.css";
import "./features/workspace/workspace.css";
import "./shared/styles/investigation.css";
const root = document.getElementById("root");
if (!root) throw new Error("Dashboard root is missing");
render(() => <App />, root);
