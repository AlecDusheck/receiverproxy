import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { probe } from "./lib/api";
import "./lib/wasm";

void probe();
mount(App, { target: document.getElementById("app")! });
