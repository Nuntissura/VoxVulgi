import { diagnosticsTrace } from "./diagnosticsTrace";

export type DiagnosticsCostClass =
  | "cheap"
  | "db_read"
  | "filesystem"
  | "python_heavy"
  | "history_replay"
  | "mutation";

export type DiagnosticsDemandState =
  | "idle"
  | "queued"
  | "loading"
  | "ready"
  | "stale"
  | "failed";

export type DiagnosticsOperationDefinition = {
  id: string;
  semanticKey: string;
  commands: readonly string[];
  ownerModules: readonly string[];
  costClass: DiagnosticsCostClass;
  trigger: "page_entry" | "section_visibility" | "module_selection" | "operator_action";
  automatic: boolean;
  freshnessMs: number;
  maxConcurrency: number;
  priority: number;
  cancellation: "non_cancellable_shared" | "backend_checkpoint";
  requestIdentity: "generated_per_owner";
};

// Product-code source of truth for every Diagnostics page-entry operation and the
// overlapping Options protection projection. Mutation entries are explicit even
// though the coordinator never schedules them automatically.
export const DIAGNOSTICS_OPERATION_REGISTRY = [
  {
    id: "diagnostics.build",
    semanticKey: "diagnostics.build",
    commands: ["diagnostics_info", "startup_status", "models_inventory", "config_batch_on_import_get", "config_diarization_optional_status", "jobs_log_retention_policy"],
    ownerModules: ["diagnostics"],
    costClass: "cheap",
    trigger: "page_entry",
    automatic: true,
    freshnessMs: 5_000,
    maxConcurrency: 4,
    priority: 100,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "diagnostics.tools-core",
    semanticKey: "diagnostics.tools-core",
    commands: ["tools_ffmpeg_status", "tools_ytdlp_status", "tools_js_runtime_status", "tools_python_status", "tools_python_portable_status", "tools_pack_integrity_manifest_status"],
    ownerModules: ["diagnostics"],
    costClass: "filesystem",
    trigger: "section_visibility",
    automatic: false,
    freshnessMs: 15_000,
    maxConcurrency: 2,
    priority: 80,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "capability.performance-tier",
    semanticKey: "capability.performance-tier",
    commands: ["tools_performance_tier_status"],
    ownerModules: ["diagnostics", "options"],
    costClass: "python_heavy",
    trigger: "section_visibility",
    automatic: false,
    freshnessMs: 0,
    maxConcurrency: 1,
    priority: 55,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "capability.demucs",
    semanticKey: "capability.demucs",
    commands: ["tools_demucs_status"],
    ownerModules: ["diagnostics", "options"],
    costClass: "python_heavy",
    trigger: "section_visibility",
    automatic: false,
    freshnessMs: 0,
    maxConcurrency: 1,
    priority: 50,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "capability.voice-backends",
    semanticKey: "capability.voice-backends",
    commands: ["voice_backends_snapshot", "voice_backend_adapters_list", "tools_spleeter_status", "tools_diarization_status", "tools_tts_preview_status", "tools_tts_neural_local_v1_status", "tools_tts_voice_preserving_local_v1_status"],
    ownerModules: ["diagnostics", "options"],
    costClass: "python_heavy",
    trigger: "section_visibility",
    automatic: false,
    freshnessMs: 0,
    maxConcurrency: 1,
    priority: 45,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "diagnostics.phase2",
    semanticKey: "diagnostics.phase2",
    commands: ["tools_phase2_packs_install_plan", "tools_phase2_packs_install_latest_state"],
    ownerModules: ["diagnostics"],
    costClass: "filesystem",
    trigger: "section_visibility",
    automatic: false,
    freshnessMs: 5_000,
    maxConcurrency: 2,
    priority: 70,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "diagnostics.storage",
    semanticKey: "diagnostics.storage",
    commands: ["diagnostics_storage_breakdown", "diagnostics_thumbnail_cache_status", "jobs_log_retention_policy", "jobs_item_artifact_retention_policy", "provider_metadata_repair_status"],
    ownerModules: ["diagnostics"],
    costClass: "filesystem",
    trigger: "section_visibility",
    automatic: false,
    freshnessMs: 10_000,
    maxConcurrency: 1,
    priority: 60,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "diagnostics.trace",
    semanticKey: "diagnostics.trace",
    commands: ["diagnostics_trace_dir_status", "diagnostics_trace_recent", "diagnostics_capture_status", "database_runtime_status"],
    ownerModules: ["diagnostics"],
    costClass: "filesystem",
    trigger: "section_visibility",
    automatic: false,
    freshnessMs: 3_000,
    maxConcurrency: 2,
    priority: 75,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "protection.snapshot",
    semanticKey: "protection.snapshot",
    commands: ["youtube_protection_snapshot_get"],
    ownerModules: ["diagnostics", "options.video_archiver"],
    costClass: "db_read",
    trigger: "section_visibility",
    automatic: false,
    freshnessMs: 5_000,
    maxConcurrency: 1,
    priority: 65,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "options.video-protection-config",
    semanticKey: "options.video-protection-config",
    commands: ["antibot_pacing_get", "youtube_protection_tuning_get"],
    ownerModules: ["options.video_archiver"],
    costClass: "db_read",
    trigger: "module_selection",
    automatic: true,
    freshnessMs: 5_000,
    maxConcurrency: 1,
    priority: 85,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "protection.history-replay",
    semanticKey: "protection.history-replay",
    commands: ["youtube_protection_history_replay"],
    ownerModules: ["diagnostics"],
    costClass: "history_replay",
    trigger: "operator_action",
    automatic: false,
    freshnessMs: 0,
    maxConcurrency: 1,
    priority: 20,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "diagnostics.operator-read",
    semanticKey: "diagnostics.operator-read",
    commands: ["diagnostics_app_state_snapshot", "diagnostics_generate_licensing_report", "jobs_cleanup_preview", "jobs_list_for_item", "jobs_queue_control_get", "jobs_runtime_settings_get", "library_get", "library_list"],
    ownerModules: ["diagnostics"],
    costClass: "db_read",
    trigger: "operator_action",
    automatic: false,
    freshnessMs: 0,
    maxConcurrency: 1,
    priority: 90,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "diagnostics.operator-mutation",
    semanticKey: "diagnostics.operator-mutation",
    commands: ["agent_freeze_dump_now", "config_diarization_optional_clear_token", "config_diarization_optional_set", "database_checkpoint_passive", "diagnostics_capture_arm", "diagnostics_capture_disarm", "diagnostics_clear_cache", "diagnostics_export_app_state_snapshot", "diagnostics_export_bundle", "diagnostics_freeze_self_test_arm", "diagnostics_thumbnail_cache_clear", "diagnostics_trace_clear", "diagnostics_trace_write_event", "jobs_enqueue_install_phase2_packs_v1", "jobs_flush_cache", "jobs_prune_logs", "models_install", "provider_metadata_repair_page", "provider_metadata_repair_reset", "tools_demucs_install", "tools_diarization_install", "tools_ffmpeg_install", "tools_js_runtime_install", "tools_pack_integrity_manifest_generate", "tools_python_install", "tools_python_portable_install", "tools_spleeter_install", "tools_tts_neural_local_v1_install", "tools_tts_preview_install", "tools_tts_voice_preserving_local_v1_install", "tools_ytdlp_install", "voice_backend_adapter_apply_starter_recipe", "voice_backend_adapter_delete", "voice_backend_adapter_probe", "voice_backend_adapter_upsert"],
    ownerModules: ["diagnostics"],
    costClass: "mutation",
    trigger: "operator_action",
    automatic: false,
    freshnessMs: 0,
    maxConcurrency: 1,
    priority: 95,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "options.protection-mutation",
    semanticKey: "options.protection-mutation",
    commands: ["antibot_pacing_set", "youtube_protection_history_export", "youtube_protection_history_reset", "youtube_protection_return_to_baseline", "youtube_protection_tuning_reset", "youtube_protection_tuning_set"],
    ownerModules: ["options.video_archiver"],
    costClass: "mutation",
    trigger: "operator_action",
    automatic: false,
    freshnessMs: 0,
    maxConcurrency: 1,
    priority: 95,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
  {
    id: "diagnostics.jobs",
    semanticKey: "diagnostics.jobs",
    commands: ["jobs_list"],
    ownerModules: ["diagnostics"],
    costClass: "db_read",
    trigger: "section_visibility",
    automatic: false,
    freshnessMs: 3_000,
    maxConcurrency: 2,
    priority: 65,
    cancellation: "non_cancellable_shared",
    requestIdentity: "generated_per_owner",
  },
] as const satisfies readonly DiagnosticsOperationDefinition[];

export type DiagnosticsOperationId = (typeof DIAGNOSTICS_OPERATION_REGISTRY)[number]["id"];

const operationById = new Map<DiagnosticsOperationId, DiagnosticsOperationDefinition>(
  DIAGNOSTICS_OPERATION_REGISTRY.map((entry) => [entry.id, entry]),
);

let demandGenerationSequence = 0;

export type DemandGeneration = {
  readonly id: number;
  readonly owner: string;
  canceled: boolean;
};

export function createDemandGeneration(owner: string): DemandGeneration {
  demandGenerationSequence += 1;
  return { id: demandGenerationSequence, owner, canceled: false };
}

export class DemandSupersededError extends Error {
  constructor(message = "Demand generation was superseded") {
    super(message);
    this.name = "DemandSupersededError";
  }
}

export type DiagnosticsDemandSnapshot = {
  operation_id: DiagnosticsOperationId;
  semantic_key: string;
  owner: string;
  generation: number;
  state: DiagnosticsDemandState;
  queued_at_ms: number | null;
  admitted_at_ms: number | null;
  verified_at_ms: number | null;
  freshness_ms: number;
  shared: boolean;
  error: string | null;
};

export type DiagnosticsDemandResult<T> = {
  value: T;
  verifiedAtMs: number;
  freshnessMs: number;
  source: "cache" | "shared" | "executed";
};

export type DiagnosticsResultTruth = {
  state: "ready" | "stale" | "failed";
  verifiedAtMs: number | null;
  error: string | null;
};

export type DiagnosticsSectionAggregate = {
  state: DiagnosticsDemandState;
  error: string | null;
  verified_at_ms: number | null;
  freshness_ms: number;
  shared: boolean;
};

export function aggregateDiagnosticsSectionSnapshots(
  snapshots: readonly DiagnosticsDemandSnapshot[],
): DiagnosticsSectionAggregate {
  const failed = snapshots.filter((entry) => entry.state === "failed");
  const hasLoading = snapshots.some((entry) => entry.state === "loading");
  const hasQueued = snapshots.some((entry) => entry.state === "queued");
  const hasStale = snapshots.some((entry) => entry.state === "stale");
  const verifiedSnapshots = snapshots.filter((entry) => entry.verified_at_ms !== null);
  const verified = verifiedSnapshots.map((entry) => entry.verified_at_ms as number);
  const freshness = verifiedSnapshots.map((entry) => entry.freshness_ms);
  const state: DiagnosticsDemandState = hasLoading
    ? "loading"
    : hasQueued
      ? "queued"
      : failed.length > 0
        ? "failed"
        : hasStale
          ? "stale"
          : snapshots.length > 0 && snapshots.every((entry) => entry.state === "ready")
            ? "ready"
            : "idle";
  return {
    state,
    error: snapshots.map((entry) => entry.error).filter(Boolean).join("; ") || null,
    verified_at_ms: verified.length > 0 ? Math.min(...verified) : null,
    freshness_ms: freshness.length > 0 ? Math.min(...freshness) : 0,
    shared: snapshots.some((entry) => entry.shared),
  };
}

export function demandGenerationOwnsCommit(
  current: DemandGeneration | null,
  candidate: DemandGeneration,
): boolean {
  return current === candidate && !candidate.canceled;
}

type CoordinatorOptions = {
  maxConcurrent?: number;
  maxPythonHeavy?: number;
  now?: () => number;
  trace?: (event: string, details?: unknown, level?: "info" | "warn" | "error") => Promise<void>;
};

type RequestOptions = {
  identity?: string;
  force?: boolean;
  onState?: (snapshot: DiagnosticsDemandSnapshot) => void;
  requestIdentity?: string;
  resultTruth?: (value: unknown) => DiagnosticsResultTruth;
};

type CacheEntry = {
  value: unknown;
  verifiedAtMs: number;
  freshnessMs: number;
};

type Waiter = {
  generation: DemandGeneration;
  requestIdentity: string;
  requestedAtMs: number;
  shared: boolean;
  onState?: (snapshot: DiagnosticsDemandSnapshot) => void;
  resolve: (result: DiagnosticsDemandResult<unknown>) => void;
  reject: (error: unknown) => void;
};

type Flight = {
  key: string;
  definition: DiagnosticsOperationDefinition;
  identity: string;
  spanIdentity: string;
  queuedAtMs: number;
  admittedAtMs: number | null;
  state: "queued" | "running";
  run: (signal: AbortSignal) => Promise<unknown>;
  abortController: AbortController;
  waiters: Set<Waiter>;
  supersededWaiters: Array<Pick<Waiter, "generation" | "requestIdentity" | "requestedAtMs" | "shared">>;
  resultTruth?: (value: unknown) => DiagnosticsResultTruth;
};

let coordinatorRequestSequence = 0;
let coordinatorFlightSequence = 0;

function nextCoordinatorRequestIdentity(generation: DemandGeneration, now: number): string {
  coordinatorRequestSequence += 1;
  return `diagnostics-demand-${generation.owner}-${generation.id}-${coordinatorRequestSequence}-${now}`;
}

function nextCoordinatorFlightIdentity(semanticKey: string, now: number): string {
  coordinatorFlightSequence += 1;
  return `diagnostics-flight-${semanticKey}-${coordinatorFlightSequence}-${now}`;
}

function childPidsFromOutcome(value: unknown): number[] {
  const found = new Set<number>();
  const visit = (candidate: unknown, depth: number) => {
    if (depth > 5 || candidate === null || candidate === undefined || found.size >= 32) return;
    if (Array.isArray(candidate)) {
      for (const entry of candidate.slice(0, 64)) visit(entry, depth + 1);
      return;
    }
    if (typeof candidate !== "object") return;
    for (const [key, entry] of Object.entries(candidate as Record<string, unknown>)) {
      if (key === "child_pid" && typeof entry === "number" && Number.isSafeInteger(entry) && entry > 0) {
        found.add(entry);
      } else if (key === "child_pids" && Array.isArray(entry)) {
        for (const pid of entry) {
          if (typeof pid === "number" && Number.isSafeInteger(pid) && pid > 0) found.add(pid);
        }
      } else {
        visit(entry, depth + 1);
      }
    }
  };
  visit(value, 0);
  return [...found].sort((left, right) => left - right);
}

export class DiagnosticsDemandCoordinator {
  private readonly maxConcurrent: number;
  private readonly maxPythonHeavy: number;
  private readonly now: () => number;
  private readonly trace: NonNullable<CoordinatorOptions["trace"]>;
  private readonly flights = new Map<string, Flight>();
  private readonly cache = new Map<string, CacheEntry>();
  private readonly queue: Flight[] = [];
  private readonly idleWaiters = new Set<() => void>();
  private activeTotal = 0;
  private activePythonHeavy = 0;
  private activeBySemanticKey = new Map<string, number>();

  constructor(options: CoordinatorOptions = {}) {
    this.maxConcurrent = Math.max(1, options.maxConcurrent ?? 4);
    this.maxPythonHeavy = Math.max(1, options.maxPythonHeavy ?? 2);
    this.now = options.now ?? Date.now;
    this.trace = options.trace ?? diagnosticsTrace;
  }

  request<T>(
    operationId: DiagnosticsOperationId,
    generation: DemandGeneration,
    run: (signal: AbortSignal) => Promise<T>,
    options: RequestOptions = {},
  ): Promise<DiagnosticsDemandResult<T>> {
    if (generation.canceled) return Promise.reject(new DemandSupersededError());
    const definition = operationById.get(operationId);
    if (!definition) return Promise.reject(new Error(`Unknown diagnostics operation: ${operationId}`));
    const identity = options.identity ?? "current-runtime";
    const key = `${definition.semanticKey}::${identity}`;
    const now = this.now();
    const requestIdentity = options.requestIdentity ?? nextCoordinatorRequestIdentity(generation, now);
    const cached = this.cache.get(key);
    if (!options.force && definition.freshnessMs > 0 && cached && now - cached.verifiedAtMs <= definition.freshnessMs) {
      const snapshot = this.snapshot(definition, generation, "ready", false, null, null, cached.verifiedAtMs, null);
      options.onState?.(snapshot);
      this.receipt(definition, generation, requestIdentity, "terminal", {
        outcome: "cache_hit",
        source: "cache",
        queue_wait_ms: 0,
        execution_ms: 0,
        shared: false,
        child_pids: childPidsFromOutcome(cached.value),
      });
      return Promise.resolve({
        value: cached.value as T,
        verifiedAtMs: cached.verifiedAtMs,
        freshnessMs: definition.freshnessMs,
        source: "cache",
      });
    }
    if (cached) {
      options.onState?.(this.snapshot(definition, generation, "stale", false, now, null, cached.verifiedAtMs, null));
    }

    const existing = this.flights.get(key);
    if (existing && options.force) {
      options.onState?.(this.snapshot(
        definition,
        generation,
        existing.state === "queued" ? "queued" : "loading",
        true,
        existing.queuedAtMs,
        existing.admittedAtMs,
        cached?.verifiedAtMs ?? null,
        null,
      ));
      return new Promise<DiagnosticsDemandResult<T>>((resolve, reject) => {
        const scheduleForcedSuccessor = () => {
          if (generation.canceled) {
            reject(new DemandSupersededError());
            return;
          }
          queueMicrotask(() => {
            if (generation.canceled) {
              reject(new DemandSupersededError());
              return;
            }
            this.cache.delete(key);
            void this.request(operationId, generation, run, {
              ...options,
              force: false,
              requestIdentity,
            }).then(resolve, reject);
          });
        };
        const waiter: Waiter = {
          generation,
          requestIdentity,
          requestedAtMs: now,
          shared: true,
          // This waiter is a freshness barrier. Do not publish the older flight's
          // ready state; publish only the forced successor's terminal state.
          onState: undefined,
          // A forced request is a freshness barrier, not a consumer of the old
          // flight's outcome. Whether the old flight succeeds or fails, schedule
          // exactly one fresh successor unless this demand generation was canceled.
          resolve: scheduleForcedSuccessor,
          reject: scheduleForcedSuccessor,
        };
        existing.waiters.add(waiter);
        this.receipt(definition, generation, requestIdentity, "shared", {
          flight_span_id: existing.spanIdentity,
          shared: true,
          force_after_current: true,
          flight_state: existing.state,
          queue_wait_ms: Math.max(0, now - existing.queuedAtMs),
        });
      });
    }
    return new Promise<DiagnosticsDemandResult<T>>((resolve, reject) => {
      const waiter: Waiter = {
        generation,
        requestIdentity,
        requestedAtMs: now,
        shared: Boolean(existing),
        onState: options.onState,
        resolve: resolve as (result: DiagnosticsDemandResult<unknown>) => void,
        reject,
      };
      if (existing) {
        existing.waiters.add(waiter);
        this.receipt(definition, generation, requestIdentity, "shared", {
          flight_span_id: existing.spanIdentity,
          shared: true,
          force_after_current: false,
          flight_state: existing.state,
          queue_wait_ms: Math.max(0, now - existing.queuedAtMs),
        });
        options.onState?.(this.snapshot(
          definition,
          generation,
          existing.state === "queued" ? "queued" : "loading",
          true,
          existing.queuedAtMs,
          existing.admittedAtMs,
          cached?.verifiedAtMs ?? null,
          null,
        ));
        return;
      }

      const flight: Flight = {
        key,
        definition,
        identity,
        spanIdentity: nextCoordinatorFlightIdentity(definition.semanticKey, now),
        queuedAtMs: now,
        admittedAtMs: null,
        state: "queued",
        run: run as (signal: AbortSignal) => Promise<unknown>,
        abortController: new AbortController(),
        waiters: new Set([waiter]),
        supersededWaiters: [],
        resultTruth: options.resultTruth,
      };
      this.flights.set(key, flight);
      this.queue.push(flight);
      options.onState?.(this.snapshot(definition, generation, "queued", false, now, null, cached?.verifiedAtMs ?? null, null));
      this.receipt(definition, generation, requestIdentity, "queued", {
        flight_span_id: flight.spanIdentity,
        shared: false,
        queue_wait_ms: 0,
        queue_depth: this.queue.length,
      });
      this.pump();
    });
  }

  cancelGeneration(generation: DemandGeneration): void {
    generation.canceled = true;
    for (const flight of this.flights.values()) {
      for (const waiter of [...flight.waiters]) {
        if (waiter.generation !== generation) continue;
        const canceledAtMs = this.now();
        this.receipt(flight.definition, generation, waiter.requestIdentity, "cancel_requested", {
          flight_span_id: flight.spanIdentity,
          shared: waiter.shared,
          flight_state: flight.state,
          queue_wait_ms: Math.max(0, (flight.admittedAtMs ?? canceledAtMs) - flight.queuedAtMs),
          execution_ms: flight.admittedAtMs === null ? 0 : Math.max(0, canceledAtMs - flight.admittedAtMs),
          cancellation: flight.definition.cancellation,
        });
        flight.waiters.delete(waiter);
        if (flight.state === "running") {
          flight.supersededWaiters.push({
            generation: waiter.generation,
            requestIdentity: waiter.requestIdentity,
            requestedAtMs: waiter.requestedAtMs,
            shared: waiter.shared,
          });
        }
        this.receipt(flight.definition, generation, waiter.requestIdentity, "waiter_detached", {
          flight_span_id: flight.spanIdentity,
          shared: waiter.shared,
          flight_state: flight.state,
          queue_wait_ms: Math.max(0, (flight.admittedAtMs ?? canceledAtMs) - flight.queuedAtMs),
          execution_ms: flight.admittedAtMs === null ? 0 : Math.max(0, canceledAtMs - flight.admittedAtMs),
          cancellation: flight.definition.cancellation,
        });
        this.receipt(flight.definition, generation, waiter.requestIdentity, "terminal", {
          flight_span_id: flight.spanIdentity,
          outcome: "superseded",
          shared: waiter.shared,
          flight_state: flight.state,
          queue_wait_ms: Math.max(0, (flight.admittedAtMs ?? canceledAtMs) - flight.queuedAtMs),
          execution_ms: flight.admittedAtMs === null ? 0 : Math.max(0, canceledAtMs - flight.admittedAtMs),
          child_pids: [],
        });
        waiter.reject(new DemandSupersededError(`${generation.owner} generation ${generation.id} was superseded`));
      }
      if (flight.waiters.size === 0 && flight.state === "queued") {
        this.removeQueuedFlight(flight);
      } else if (flight.waiters.size === 0 && flight.definition.cancellation === "backend_checkpoint") {
        flight.abortController.abort();
        for (const waiter of flight.supersededWaiters) {
          this.receipt(flight.definition, waiter.generation, waiter.requestIdentity, "frontend_abort_signaled", {
            flight_span_id: flight.spanIdentity,
            backend_acknowledged: false,
          });
        }
      }
    }
    this.pump();
    this.resolveIdleIfNeeded();
  }

  invalidate(operationId: DiagnosticsOperationId, identity?: string): void {
    const definition = operationById.get(operationId);
    if (!definition) return;
    if (identity !== undefined) {
      this.cache.delete(`${definition.semanticKey}::${identity}`);
      return;
    }
    for (const key of [...this.cache.keys()]) {
      if (key.startsWith(`${definition.semanticKey}::`)) this.cache.delete(key);
    }
  }

  whenIdle(): Promise<void> {
    if (this.activeTotal === 0 && this.queue.length === 0) return Promise.resolve();
    return new Promise((resolve) => this.idleWaiters.add(resolve));
  }

  private pump(): void {
    while (this.activeTotal < this.maxConcurrent) {
      const nextIndex = this.nextAdmissibleIndex();
      if (nextIndex < 0) break;
      const [flight] = this.queue.splice(nextIndex, 1);
      if (!flight || flight.waiters.size === 0) {
        if (flight) this.flights.delete(flight.key);
        continue;
      }
      this.startFlight(flight);
    }
    this.resolveIdleIfNeeded();
  }

  private nextAdmissibleIndex(): number {
    const now = this.now();
    let selected = -1;
    let selectedScore = Number.NEGATIVE_INFINITY;
    for (let index = 0; index < this.queue.length; index += 1) {
      const flight = this.queue[index];
      const semanticActive = this.activeBySemanticKey.get(flight.definition.semanticKey) ?? 0;
      if (semanticActive >= flight.definition.maxConcurrency) continue;
      if (flight.definition.costClass === "python_heavy" && this.activePythonHeavy >= this.maxPythonHeavy) continue;
      const ageSeconds = Math.floor(Math.max(0, now - flight.queuedAtMs) / 1_000);
      const score = flight.definition.priority + ageSeconds;
      if (score > selectedScore) {
        selected = index;
        selectedScore = score;
      }
    }
    return selected;
  }

  private startFlight(flight: Flight): void {
    flight.state = "running";
    flight.admittedAtMs = this.now();
    this.activeTotal += 1;
    if (flight.definition.costClass === "python_heavy") this.activePythonHeavy += 1;
    this.activeBySemanticKey.set(
      flight.definition.semanticKey,
      (this.activeBySemanticKey.get(flight.definition.semanticKey) ?? 0) + 1,
    );
    for (const waiter of flight.waiters) {
      this.receipt(flight.definition, waiter.generation, waiter.requestIdentity, "admitted", {
        flight_span_id: flight.spanIdentity,
        shared: waiter.shared,
        queue_wait_ms: Math.max(0, flight.admittedAtMs - waiter.requestedAtMs),
        flight_queue_wait_ms: Math.max(0, flight.admittedAtMs - flight.queuedAtMs),
        active_total: this.activeTotal,
        active_python_heavy: this.activePythonHeavy,
      });
      waiter.onState?.(this.snapshot(
        flight.definition,
        waiter.generation,
        "loading",
        waiter.shared,
        flight.queuedAtMs,
        flight.admittedAtMs,
        this.cache.get(flight.key)?.verifiedAtMs ?? null,
        null,
      ));
    }

    // Normalize both synchronous throws and asynchronous rejections through the same terminal
    // cleanup path. Calling `run` directly can otherwise strand active counters forever.
    void Promise.resolve().then(() => flight.run(flight.abortController.signal)).then(
      (value) => this.finishFlight(flight, true, value),
      (error) => this.finishFlight(flight, false, error),
    );
  }

  private finishFlight(flight: Flight, succeeded: boolean, outcome: unknown): void {
    const finishedAtMs = this.now();
    const truth = succeeded
      ? flight.resultTruth?.(outcome) ?? { state: "ready" as const, verifiedAtMs: finishedAtMs, error: null }
      : null;
    const verifiedAtMs = truth?.verifiedAtMs ?? null;
    const childPids = succeeded ? childPidsFromOutcome(outcome) : [];
    const executionMs = flight.admittedAtMs === null ? 0 : Math.max(0, finishedAtMs - flight.admittedAtMs);
    const flightQueueWaitMs = flight.admittedAtMs === null ? 0 : Math.max(0, flight.admittedAtMs - flight.queuedAtMs);
    if (succeeded && truth?.state === "ready" && verifiedAtMs !== null && flight.definition.freshnessMs > 0) {
      this.cache.set(flight.key, {
        value: outcome,
        verifiedAtMs,
        freshnessMs: flight.definition.freshnessMs,
      });
    }
    for (const waiter of flight.waiters) {
      if (waiter.generation.canceled) {
        this.receipt(flight.definition, waiter.generation, waiter.requestIdentity, "superseded_completion", {
          flight_span_id: flight.spanIdentity,
          outcome: !succeeded || truth?.state === "failed"
            ? "failed_after_supersession"
            : truth?.state === "stale"
              ? "stale_after_supersession"
              : "completed_after_supersession",
          shared: waiter.shared,
          queue_wait_ms: flightQueueWaitMs,
          execution_ms: executionMs,
          child_pids: childPids,
          error: succeeded ? truth?.error ?? null : String(outcome).slice(0, 500),
        }, !succeeded || truth?.state === "failed" ? "error" : "warn");
        waiter.reject(new DemandSupersededError());
        continue;
      }
      if (!succeeded) {
        waiter.onState?.(this.snapshot(
          flight.definition,
          waiter.generation,
          "failed",
          waiter.shared,
          flight.queuedAtMs,
          flight.admittedAtMs,
          null,
          String(outcome),
        ));
        this.receipt(flight.definition, waiter.generation, waiter.requestIdentity, "terminal", {
          flight_span_id: flight.spanIdentity,
          outcome: "failed",
          shared: waiter.shared,
          queue_wait_ms: flightQueueWaitMs,
          execution_ms: executionMs,
          child_pids: childPids,
          error: String(outcome).slice(0, 500),
        }, "error");
        waiter.reject(outcome);
        continue;
      }
      waiter.onState?.(this.snapshot(
        flight.definition,
        waiter.generation,
        truth?.state ?? "ready",
        waiter.shared,
        flight.queuedAtMs,
        flight.admittedAtMs,
        verifiedAtMs,
        truth?.error ?? null,
      ));
      this.receipt(flight.definition, waiter.generation, waiter.requestIdentity, "terminal", {
        flight_span_id: flight.spanIdentity,
        outcome: truth?.state === "ready" ? "succeeded" : `probe_${truth?.state ?? "failed"}`,
        shared: waiter.shared,
        queue_wait_ms: flightQueueWaitMs,
        execution_ms: executionMs,
        child_pids: childPids,
        error: truth?.error ?? null,
      }, truth?.state === "failed" ? "error" : truth?.state === "stale" ? "warn" : "info");
      waiter.resolve({
        value: outcome,
        verifiedAtMs: verifiedAtMs ?? 0,
        freshnessMs: flight.definition.freshnessMs,
        source: waiter.shared ? "shared" : "executed",
      });
    }
    for (const superseded of flight.supersededWaiters) {
      this.receipt(flight.definition, superseded.generation, superseded.requestIdentity, "superseded_completion", {
        flight_span_id: flight.spanIdentity,
        outcome: !succeeded || truth?.state === "failed"
          ? "failed_after_supersession"
          : truth?.state === "stale"
            ? "stale_after_supersession"
            : "completed_after_supersession",
        shared: superseded.shared,
        queue_wait_ms: flightQueueWaitMs,
        execution_ms: executionMs,
        child_pids: childPids,
        error: succeeded ? truth?.error ?? null : String(outcome).slice(0, 500),
      }, !succeeded || truth?.state === "failed" ? "error" : "warn");
    }
    flight.supersededWaiters.length = 0;
    flight.waiters.clear();
    this.flights.delete(flight.key);
    this.activeTotal = Math.max(0, this.activeTotal - 1);
    if (flight.definition.costClass === "python_heavy") {
      this.activePythonHeavy = Math.max(0, this.activePythonHeavy - 1);
    }
    const semanticActive = Math.max(
      0,
      (this.activeBySemanticKey.get(flight.definition.semanticKey) ?? 1) - 1,
    );
    if (semanticActive === 0) this.activeBySemanticKey.delete(flight.definition.semanticKey);
    else this.activeBySemanticKey.set(flight.definition.semanticKey, semanticActive);
    this.pump();
  }

  private removeQueuedFlight(flight: Flight): void {
    const index = this.queue.indexOf(flight);
    if (index >= 0) this.queue.splice(index, 1);
    this.flights.delete(flight.key);
  }

  private receipt(
    definition: DiagnosticsOperationDefinition,
    generation: DemandGeneration,
    requestIdentity: string,
    phase: "queued" | "admitted" | "shared" | "cancel_requested" | "waiter_detached" | "frontend_abort_signaled" | "backend_cancel_observed" | "terminal" | "superseded_completion",
    details: Record<string, unknown>,
    level: "info" | "warn" | "error" = "info",
  ): void {
    void this.trace("diagnostics_demand_scheduler", {
      phase,
      operation_id: definition.id,
      semantic_key: definition.semanticKey,
      span_id: typeof details.flight_span_id === "string"
        ? details.flight_span_id
        : `diagnostics-request:${requestIdentity}`,
      request_identity: requestIdentity,
      owner: generation.owner,
      generation: generation.id,
      cost_class: definition.costClass,
      cancellation: definition.cancellation,
      ...details,
    }, level).catch(() => undefined);
  }

  private snapshot(
    definition: DiagnosticsOperationDefinition,
    generation: DemandGeneration,
    state: DiagnosticsDemandState,
    shared: boolean,
    queuedAtMs: number | null,
    admittedAtMs: number | null,
    verifiedAtMs: number | null,
    error: string | null,
  ): DiagnosticsDemandSnapshot {
    return {
      operation_id: definition.id as DiagnosticsOperationId,
      semantic_key: definition.semanticKey,
      owner: generation.owner,
      generation: generation.id,
      state,
      queued_at_ms: queuedAtMs,
      admitted_at_ms: admittedAtMs,
      verified_at_ms: verifiedAtMs,
      freshness_ms: definition.freshnessMs,
      shared,
      error,
    };
  }

  private resolveIdleIfNeeded(): void {
    if (this.activeTotal !== 0 || this.queue.length !== 0) return;
    for (const resolve of this.idleWaiters) resolve();
    this.idleWaiters.clear();
  }
}

export const diagnosticsDemandCoordinator = new DiagnosticsDemandCoordinator();
