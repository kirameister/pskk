/**
 * Typed wrappers around the Tauri command surface.
 *
 * Tauriコマンド呼び出しの型付きラッパー。
 * When running outside Tauri (plain `vite dev` in a browser) every call
 * rejects with a readable error instead of a cryptic one.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CorpusReport,
  EnvironmentInfo,
  ExtractRequest,
  FeatureExtractionResult,
  FeatureTsvReport,
  ModelInfo,
  ModelTargetInfo,
  PredictRequest,
  PredictionResult,
  ProgressEvent,
  TrainRequest,
  TrainingParams,
  TrainingResult,
} from "./types";

/** Tauri v2 injects this global; its absence means "plain browser". */
function assertTauri(): void {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    throw new Error(
      "Tauri host not detected. Run `just crf-trainer-dev` (or `cargo tauri dev`) instead of opening the Vite URL directly."
    );
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  assertTauri();
  return invoke<T>(command, args);
}

// ─── Environment / models ────────────────────────────────────────────

export const getEnvironment = (): Promise<EnvironmentInfo> =>
  call<EnvironmentInfo>("get_environment");

export const listModels = (): Promise<ModelInfo[]> => call<ModelInfo[]>("list_models");

/**
 * Report whether the training output path already holds a model file.
 * `path` may be null/empty to mean "the IME's model path".
 */
export const checkModelTarget = (path?: string | null): Promise<ModelTargetInfo> =>
  call<ModelTargetInfo>("check_model_target", { path: path ?? null });

/**
 * Ask the user to confirm overwriting an existing model file.
 * Returns true when training may proceed (nothing there, or the user agreed).
 */
export const confirmModelOverwrite = (path?: string | null): Promise<boolean> =>
  call<boolean>("confirm_model_overwrite", { path: path ?? null });

// ─── File dialogs ────────────────────────────────────────────────────

export const pickCorpusFile = (): Promise<string | null> =>
  call<string | null>("pick_corpus_file");

export const pickModelFile = (): Promise<string | null> =>
  call<string | null>("pick_model_file");

export const pickSaveFile = (
  title: string,
  defaultPath?: string | null,
  extension?: string | null
): Promise<string | null> =>
  call<string | null>("pick_save_file", {
    title,
    defaultPath: defaultPath ?? null,
    extension: extension ?? null,
  });

// ─── Training parameters ─────────────────────────────────────────────

export const loadTrainingParams = (): Promise<TrainingParams> =>
  call<TrainingParams>("load_training_params");

export const saveTrainingParams = (params: TrainingParams): Promise<void> =>
  call<void>("save_training_params", { params });

// ─── Corpus ──────────────────────────────────────────────────────────

export const loadCorpusReport = (
  paths: string[],
  sampleLimit = 20
): Promise<CorpusReport> =>
  call<CorpusReport>("load_corpus_report", { paths, sampleLimit });

export const inspectFeatureTsv = (
  path: string,
  limit = 20
): Promise<FeatureTsvReport> =>
  call<FeatureTsvReport>("inspect_feature_tsv", { path, limit });

// ─── CRF pipeline (pure Rust, in-process) ────────────────────────────

export const extractFeatures = (
  request: ExtractRequest
): Promise<FeatureExtractionResult> =>
  call<FeatureExtractionResult>("extract_features", { request });

export const trainModel = (request: TrainRequest): Promise<TrainingResult> =>
  call<TrainingResult>("train_model", { request });

export const predict = (request: PredictRequest): Promise<PredictionResult> =>
  call<PredictionResult>("predict", { request });

export const displayFeatureKeys = (): Promise<string[]> =>
  call<string[]>("display_feature_keys");

// ─── Progress stream ─────────────────────────────────────────────────

/**
 * Subscribe to `crf-progress` events emitted by extraction/training jobs.
 * 抽出・訓練ジョブが発行する`crf-progress`イベントを購読する。
 */
export function onProgress(handler: (event: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>("crf-progress", (event) => handler(event.payload));
}

// ─── Formatting helpers ──────────────────────────────────────────────

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "—";
  const units = ["B", "KB", "MB", "GB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${value.toFixed(exponent === 0 ? 0 : 1)} ${units[exponent]}`;
}

/** Render the corpus `modified` epoch-seconds string as a local date. */
export function formatEpochSeconds(raw: string | null): string {
  if (!raw) return "—";
  const seconds = Number(raw);
  if (!Number.isFinite(seconds) || seconds <= 0) return raw;
  return new Date(seconds * 1000).toLocaleString();
}
