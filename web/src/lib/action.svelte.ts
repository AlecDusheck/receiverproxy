// One action's four states (docs/ui-design.md, "States"): idle, busy (control
// disabled), done (result where the action was), error (message under the
// action, verbatim).
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
    try {
      const r = await f();
      this.result = r;
      this.state = "done";
      return r;
    } catch (e) {
      this.error = errText(e);
      this.state = "error";
      return null;
    }
  }
}
