import type { Phase2TransferStatus } from "./usePhase2Transfer";

export type Phase2RestoreLatest = {
  canonical_install?: { id: string } | null;
  state?: { job_id?: unknown } | null;
};

type CanonicalInstall = NonNullable<Phase2TransferStatus["canonical_install"]>;

/** Journal identity is a lookup hint; only the exact canonical job can restore UI state. */
export async function restorePhase2Install(
  readLatest: () => Promise<Phase2RestoreLatest>,
  readExact: (id: string) => Promise<Phase2TransferStatus>,
  commit: (job: CanonicalInstall) => void,
  mayCommit: () => boolean,
): Promise<void> {
  if (!mayCommit()) return;
  const latest = await readLatest();
  if (!mayCommit()) return;
  const id = latest.canonical_install?.id ?? latest.state?.job_id;
  if (typeof id !== "string" || !id.trim()) return;
  const receipt = await readExact(id);
  if (!mayCommit()) return;
  const job = receipt.canonical_install;
  if (!job || job.id !== id || job.job_type !== "install_phase2_packs_v1") return;
  if (job.status === "queued" || job.status === "running" || job.status === "failed") commit(job);
}
