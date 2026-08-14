export type InstagramCapabilityEpochRef = { current: number };

export function beginInstagramCapabilityEpoch(ref: InstagramCapabilityEpochRef): number {
  ref.current += 1;
  return ref.current;
}

export function captureInstagramCapabilityEpoch(ref: InstagramCapabilityEpochRef): number {
  return ref.current;
}

export function invalidateInstagramCapabilityEpoch(ref: InstagramCapabilityEpochRef): number {
  ref.current += 1;
  return ref.current;
}

export function isCurrentInstagramCapabilityEpoch(
  ref: InstagramCapabilityEpochRef,
  capturedEpoch: number,
): boolean {
  return ref.current === capturedEpoch;
}

// Credential mutations have a separate settlement lifetime from capability/preflight work.
// Navigation may invalidate a capability receipt, but it must not hide a committed mutation
// warning or leave the mutation controls busy forever.
export const beginInstagramMutationEpoch = beginInstagramCapabilityEpoch;
export const isCurrentInstagramMutationEpoch = isCurrentInstagramCapabilityEpoch;

export function applyIfCurrentInstagramMutation(
  ref: InstagramCapabilityEpochRef,
  capturedEpoch: number,
  apply: () => void,
): boolean {
  if (!isCurrentInstagramMutationEpoch(ref, capturedEpoch)) return false;
  apply();
  return true;
}

export type InstagramCredentialRevision = {
  generation: number;
  fingerprint: string;
};

export function isCurrentInstagramCredentialRevision(
  expected: InstagramCredentialRevision | null,
  receipt: { credential_generation: number; credential_fingerprint: string },
): boolean {
  return expected != null &&
    expected.generation === receipt.credential_generation &&
    expected.fingerprint === receipt.credential_fingerprint;
}
