import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { addTransferTransportAge, beginTransferExpiry, expireTransferRate, phase2TransferView, type Phase2Transfer, type CanonicalTransferOwner } from "./phase2Transfer";
export type Phase2TransferStatus = {
  canonical_install: {id: string; job_type: string; attempt_no: number; status: "queued" | "running" | "succeeded" | "failed" | "canceled"; progress: number; error: string | null} | null;
  owner: CanonicalTransferOwner | null;
  transfer: Phase2Transfer | null;
};
export function beginPhase2TransferPolling(jobId: string, readReceipt: () => Promise<Phase2TransferStatus>, commit: (value: Phase2TransferStatus | null) => void, schedule: (run: () => void) => () => void, clock: () => number = () => performance.now()) {
  let live = true;
  let clear: (() => void) | null = null;
  const read = async () => {
    let terminal = false;
    try {
      const started=clock();
      const next = await readReceipt();
      const elapsed=clock()-started;
      if (!live) return;
      const matching = next.canonical_install?.id === jobId ? {...next,transfer:addTransferTransportAge(next.transfer,elapsed)} : null;
      commit(matching);
      terminal = matching !== null && ["succeeded", "failed", "canceled"].includes(matching.canonical_install!.status);
    } catch {
      if (live) commit(null);
    } finally {
      if (live && !terminal) clear = schedule(() => {void read();});
    }
  };
  void read();
  return () => {live = false;clear?.();};
}
/** Shared generation-owned exact job read; no loaded/paginated Jobs rows. */
export function usePhase2Transfer(jobId: string | null, visible: boolean) {
  const [observed, setObserved] = useState<{receipt:Phase2TransferStatus | null; receivedAt:number}>({receipt:null,receivedAt:0});
  const [, expire] = useState(0);
  const receipt=observed.receipt;
  useEffect(() => {
    setObserved({receipt:null,receivedAt:performance.now()});
    if (!jobId || !visible) return;
    return beginPhase2TransferPolling(jobId, () => invoke<Phase2TransferStatus>("tools_phase2_transfer_status", {jobId}), (receipt) => setObserved({receipt,receivedAt:performance.now()}),
      (run) => {const timer = setTimeout(run, 1000);return () => clearTimeout(timer);});
  }, [jobId, visible]);
  useEffect(() => beginTransferExpiry(receipt?.transfer ?? null, () => expire((n)=>n+1),
    (run,delay) => {const timer=setTimeout(run,delay);return ()=>clearTimeout(timer);}), [observed]);
  const matching = visible && receipt?.canonical_install?.id === jobId ? receipt : null;
  return {receipt: matching, view: phase2TransferView(expireTransferRate(matching?.transfer ?? null,performance.now()-observed.receivedAt), matching?.owner ?? null)};
}
