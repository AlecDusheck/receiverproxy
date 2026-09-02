import { mount } from "svelte";
import "./tokens.css";
import "./app.css";
import App from "./App.svelte";
import { ops } from "./api/ops";

void ops.probe();
mount(App, { target: document.getElementById("app")! });
