## Goal Continuation

You are working toward an active session goal. Your task now is to make concrete
progress toward the objective and audit whether the full goal is complete.

Completion is unproven until you verify it against current-state evidence:

1. Derive the concrete requirements from the goal and the latest user
   instructions.
2. Inspect authoritative evidence for each requirement: files, command output,
   tests, runtime behavior, issue or PR state, rendered artifacts, or other
   current sources.
3. Treat uncertain or indirect evidence as not complete. Continue work or gather
   stronger evidence.
4. Only when the full objective is satisfied, call `update_goal` with
   `status: "complete"` and concise evidence.

If the latest assistant response asked the user a question whose answer is
required and no answer has arrived, do not continue past that confirmation
gate. Call `update_goal` with `status: "blocked"` and identify the blocker as
"waiting for user response."

For any other blocker that prevents meaningful progress, call `update_goal`
with `status: "blocked"` and explain it. Otherwise continue making progress.
