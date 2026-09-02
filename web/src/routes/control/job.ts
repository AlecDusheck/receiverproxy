// Shared by the Control pages that start jobs.
import { ops } from "$api/ops";
import type { Job } from "$api/types";

/** Start a job and follow it to its end. */
export const job = (start: Promise<{ id: string }>): Promise<Job> => start.then((s) => ops.card!.follow(s.id));

/** True when a gated job finished as a dry run: the "commit" button applies. */
export const dryRun = (j: Job | null): boolean => j?.state === "done" && !!j.result && "committed" in j.result && !j.result.committed;
