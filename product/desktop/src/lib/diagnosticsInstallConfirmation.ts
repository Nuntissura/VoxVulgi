export type DiagnosticsInstallPrompt = { title: string; message: string };

export function createDiagnosticsInstallConfirmation(
  available: () => boolean,
  show: (prompt: DiagnosticsInstallPrompt | null) => void,
) {
  let pending: ((approved: boolean) => void) | null = null;
  function resolve(approved: boolean) {
    const continuation = pending;
    pending = null;
    if (!continuation) return;
    show(null);
    continuation(approved && available());
  }
  return {
    request(prompt: DiagnosticsInstallPrompt): Promise<boolean> {
      if (pending || !available()) return Promise.resolve(false);
      return new Promise((continuation) => { pending = continuation; show(prompt); });
    },
    resolve,
    cancel: () => resolve(false),
    canStart: () => !pending && available(),
  };
}
