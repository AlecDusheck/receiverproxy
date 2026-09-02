// One action's four states (docs/ui-design.md, "States"): idle, busy (control
// disabled, progress in the status bar), done (result where the action was,
// status bar cleared), error (message under the action, verbatim, and the same
// line in the status bar).
import { app, setStatus } from "./state.svelte";
import { errText } from "./error";

export type ActionState = "idle" | "busy" | "done" | "error";

export class Action<T> {
  state = $state<ActionState>("idle");
  error = $state("");
  result = $state<T | null>(null);
  readonly label: string;

  constructor(label: string) {
    this.label = label;
  }

  get busy() {
    return this.state === "busy";
  }

  reset() {
    this.state = "idle";
    this.error = "";
    this.result = null;
  }

  async run(f: () => Promise<T>): Promise<T | null> {
    this.state = "busy";
    this.error = "";
    setStatus("busy", this.label);
    try {
      const r = await f();
      this.result = r;
      this.state = "done";
      if (app.status.kind !== "error") setStatus("idle");
      return r;
    } catch (e) {
      this.error = errText(e);
      this.state = "error";
      setStatus("error", this.error);
      return null;
    }
  }
}
