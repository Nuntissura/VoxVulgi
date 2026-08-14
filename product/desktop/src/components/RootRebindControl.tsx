import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type RootRebindTaskStatus = {
  task_id: string;
  operation: string;
  state: "queued" | "running" | "completed" | "failed";
  submitted_at_ms: number;
  started_at_ms: number | null;
  finished_at_ms: number | null;
  result: unknown | null;
  error: string | null;
};

type RootRebindReceipt = {
  id: string;
  from_root: string;
  to_root: string;
  status: string;
  phase: string;
  dry_run: Record<string, number>;
  updated_at_ms: number;
};

function pretty(value: unknown): string {
  return value == null ? "" : JSON.stringify(value, null, 2);
}

export function RootRebindControl() {
  const [fromRoot, setFromRoot] = useState("");
  const [toRoot, setToRoot] = useState("");
  const [receiptId, setReceiptId] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [task, setTask] = useState<RootRebindTaskStatus | null>(null);
  const [receipts, setReceipts] = useState<RootRebindReceipt[]>([]);
  const [message, setMessage] = useState("");
  const busy = task?.state === "queued" || task?.state === "running";

  const acceptTicket = useCallback((ticket: RootRebindTaskStatus) => {
    setTask(ticket);
    setMessage("");
  }, []);

  useEffect(() => {
    if (!task || (task.state !== "queued" && task.state !== "running")) return;
    const timer = window.setTimeout(() => {
      void invoke<RootRebindTaskStatus>("root_rebind_task_status", {
        taskId: task.task_id,
        waitTimeoutMs: null,
      })
        .then((next) => {
          setTask(next);
          if (next.state === "failed") setMessage(next.error ?? "Root rebind task failed.");
          if (next.state === "completed" && next.operation === "prepare") {
            const result = next.result as Partial<RootRebindReceipt> | null;
            if (result?.id) setReceiptId(result.id);
          }
        })
        .catch((error) => setMessage(String(error)));
    }, 250);
    return () => window.clearTimeout(timer);
  }, [task]);

  const queue = useCallback(
    async (command: string, args: Record<string, unknown>) => {
      try {
        acceptTicket(await invoke<RootRebindTaskStatus>(command, args));
      } catch (error) {
        setMessage(String(error));
      }
    },
    [acceptTicket],
  );

  const inspectReceipts = useCallback(async () => {
    try {
      const rows = await invoke<RootRebindReceipt[]>("root_rebind_status", {
        receiptId: receiptId.trim() || null,
      });
      setReceipts(rows);
      setMessage("");
    } catch (error) {
      setMessage(String(error));
    }
  }, [receiptId]);

  return (
    <details data-testid="root-rebind-control" style={{ marginTop: 18 }}>
      <summary style={{ cursor: "pointer", fontWeight: 650 }}>Archive root rebind</summary>
      <p style={{ color: "#4b5563", maxWidth: 900 }}>
        Rebind stored destinations after an archive root moves. Dry-run and prepare are read-only;
        prepare verifies canonical files from the old and new roots and creates independent
        backups. Apply and rollback require the exact confirmation shown below. Historical media
        identities are preserved, and this tool never performs an MP4 cleanup or conversion.
      </p>
      <div className="row" style={{ flexWrap: "wrap", alignItems: "end" }}>
        <label style={{ minWidth: 280, flex: 1 }}>
          Existing root
          <input
            data-testid="root-rebind-from"
            value={fromRoot}
            onChange={(event) => setFromRoot(event.target.value)}
            placeholder="Existing configured archive root"
          />
        </label>
        <label style={{ minWidth: 280, flex: 1 }}>
          New root
          <input
            data-testid="root-rebind-to"
            value={toRoot}
            onChange={(event) => setToRoot(event.target.value)}
            placeholder="New directly connected archive root"
          />
        </label>
      </div>
      <div className="row" style={{ flexWrap: "wrap" }}>
        <button
          type="button"
          data-testid="root-rebind-dry-run"
          disabled={busy || !fromRoot.trim()}
          onClick={() => void queue("root_rebind_dry_run", { fromRoot })}
        >
          Inspect dry-run
        </button>
        <button
          type="button"
          data-testid="root-rebind-prepare"
          disabled={busy || !fromRoot.trim() || !toRoot.trim()}
          onClick={() =>
            void queue("root_rebind_prepare", { fromRoot, toRoot, evidence: [] })
          }
        >
          Verify and prepare
        </button>
        <button
          type="button"
          data-testid="root-rebind-recover"
          disabled={busy}
          onClick={() => void queue("root_rebind_recover", {})}
        >
          Reconcile interrupted operation
        </button>
        {busy && (task?.operation === "prepare" || task?.operation === "apply") ? (
          <button
            type="button"
            data-testid="root-rebind-cancel"
            onClick={() => {
              if (!task) return;
              void invoke<RootRebindTaskStatus>("root_rebind_task_cancel", {
                taskId: task.task_id,
              })
                .then(setTask)
                .catch((error) => setMessage(String(error)));
            }}
          >
            Cancel storage probe
          </button>
        ) : null}
      </div>
      <div className="row" style={{ flexWrap: "wrap", alignItems: "end" }}>
        <label style={{ minWidth: 300, flex: 1 }}>
          Receipt ID
          <input
            data-testid="root-rebind-receipt-id"
            value={receiptId}
            onChange={(event) => setReceiptId(event.target.value)}
            placeholder="root-rebind-..."
          />
        </label>
        <button type="button" disabled={busy} onClick={() => void inspectReceipts()}>
          Inspect receipt
        </button>
      </div>
      <div style={{ fontSize: 12, marginTop: 8 }}>
        Apply confirmation: <code>APPLY:{receiptId || "<receipt-id>"}</code>
        <br />
        Rollback confirmation: <code>ROLLBACK:{receiptId || "<receipt-id>"}</code>
      </div>
      <div className="row" style={{ flexWrap: "wrap", alignItems: "end" }}>
        <label style={{ minWidth: 360, flex: 1 }}>
          Exact confirmation
          <input
            data-testid="root-rebind-confirmation"
            value={confirmation}
            onChange={(event) => setConfirmation(event.target.value)}
          />
        </label>
        <button
          type="button"
          data-testid="root-rebind-apply"
          disabled={busy || !receiptId.trim() || confirmation !== `APPLY:${receiptId.trim()}`}
          onClick={() =>
            void queue("root_rebind_apply", { receiptId: receiptId.trim(), confirmation })
          }
        >
          Apply prepared rebind
        </button>
        <button
          type="button"
          data-testid="root-rebind-rollback"
          disabled={busy || !receiptId.trim() || confirmation !== `ROLLBACK:${receiptId.trim()}`}
          onClick={() =>
            void queue("root_rebind_rollback", { receiptId: receiptId.trim(), confirmation })
          }
        >
          Roll back receipt
        </button>
      </div>
      {message ? <div role="alert" style={{ color: "#b42318", marginTop: 8 }}>{message}</div> : null}
      {task ? (
        <div data-testid="root-rebind-task-status" style={{ marginTop: 12 }}>
          <strong>Task:</strong> {task.operation} · {task.state} · {task.task_id}
          <pre style={{ maxHeight: 240, overflow: "auto", whiteSpace: "pre-wrap" }}>
            {pretty(task.result ?? task.error)}
          </pre>
        </div>
      ) : null}
      {receipts.length ? (
        <pre
          data-testid="root-rebind-receipts"
          style={{ maxHeight: 280, overflow: "auto", whiteSpace: "pre-wrap" }}
        >
          {pretty(receipts)}
        </pre>
      ) : null}
    </details>
  );
}
