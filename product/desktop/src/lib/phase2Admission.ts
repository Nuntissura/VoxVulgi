export type Phase2Admission = { id: string; attempt_no: number; force: boolean; held: boolean; observedAtMs: number };
export type Phase2CanonicalInstall = { id: string; attempt_no: number; status: string; force: boolean; held: boolean };
export const PHASE2_JOURNAL_WAIT_MS = 180_000;

export function phase2AdmissionView(
  admission: Phase2Admission | null,
  canonical: Phase2CanonicalInstall | null | undefined,
  journalId: unknown,
  nowMs: number,
) {
  if (!admission) return { poll: false, previous: false, terminal: false, label: null };
  const exact = canonical?.id === admission.id && canonical.attempt_no === admission.attempt_no ? canonical : null;
  const terminal = !!exact && ["succeeded", "failed", "canceled"].includes(exact.status);
  const previous = journalId !== admission.id;
  if (terminal) return { poll: false, previous, terminal: true, label: previous ? `Installer ${admission.id} ${exact.status}; its journal is not available.` : null };
  const running = exact?.status === "running";
  const held = !running && (exact ? exact.held : admission.held);
  const expired = !exact && nowMs - admission.observedAtMs >= PHASE2_JOURNAL_WAIT_MS;
  return {
    poll: !expired,
    previous,
    terminal: false,
    label: expired ? `Installer ${admission.id} attempt ${admission.attempt_no}: canonical observation timed out; Refresh to discover current installer state. No job was stopped.`
      : held ? `Installer ${admission.id} is queued and held; previous history is retained.`
      : previous ? `Installer ${admission.id} is ${running ? "running" : "queued"}; waiting for its own installation journal.` : null,
  };
}
