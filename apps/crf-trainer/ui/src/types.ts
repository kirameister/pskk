/**
 * TypeScript mirrors of the Rust DTOs in `src-tauri/src/types.rs`.
 *
 * `src-tauri/src/types.rs` のRust DTOに対応するTypeScript型定義。
 * Keep these in sync when the command surface changes.
 */

// ─── Environment ─────────────────────────────────────────────────────

export interface EnvironmentInfo {
  pythonAvailable: boolean;
  pythonVersion: string | null;
  pycrfsuiteAvailable: boolean;
  crfsuiteVersion: string | null;
  configDir: string;
  defaultModelPath: string;
  defaultFeaturesPath: string;
}

// ─── Models ──────────────────────────────────────────────────────────

export interface ModelInfo {
  path: string;
  fileName: string;
  sizeBytes: number;
  modified: string | null;
  isDefault: boolean;
}

/** What a training run would write to, and whether that clobbers something. */
export interface ModelTargetInfo {
  path: string;
  exists: boolean;
  sizeBytes: number;
  modified: string | null;
  /** True for `~/.config/pskk/bunsetsu.crfsuite` — the model the IME loads. */
  isLiveModelPath: boolean;
  /** True for a model shipped under the install root's data dir. */
  isShippedModel: boolean;
}

// ─── Corpus ──────────────────────────────────────────────────────────

export interface CorpusStats {
  source: string;
  lineCount: number;
  sentenceCount: number;
  totalTokens: number;
  totalBunsetsu: number;
  lookupBunsetsu: number;
  passthroughBunsetsu: number;
  totalChars: number;
}

export interface Bunsetsu {
  text: string;
  /** `L` = lookup (dictionary conversion), `P` = passthrough. */
  kind: string;
  isLookup: boolean;
}

export interface SampleSentence {
  lineNumber: number;
  raw: string;
  tokens: string[];
  labels: string[];
  bunsetsu: Bunsetsu[];
}

export interface CorpusReport {
  perFile: CorpusStats[];
  combined: CorpusStats;
  samples: SampleSentence[];
  samplesTruncated: boolean;
  warnings: string[];
}

// ─── Features TSV inspector ──────────────────────────────────────────

export interface FeatureTsvSentence {
  index: number;
  lineNumber: number;
  tokens: string[];
  labels: string[];
  bunsetsu: Bunsetsu[];
}

export interface LabelCount {
  label: string;
  count: number;
}

export interface FeatureTsvReport {
  path: string;
  sizeBytes: number;
  sentenceCount: number;
  tokenCount: number;
  featureKeys: string[];
  labelCounts: LabelCount[];
  sentences: FeatureTsvSentence[];
  truncated: boolean;
  warnings: string[];
}

// ─── Feature extraction ──────────────────────────────────────────────

export interface ExtractRequest {
  corpusPaths: string[];
  outputPath: string | null;
  includeDictionary: boolean;
  regenerateDictionaryFeatures: boolean;
}

export interface FeatureExtractionResult {
  success: boolean;
  outputPath: string | null;
  tsvSizeBytes: number;
  stats: CorpusStats;
  dictEntries: number;
  dictTokens: number;
  elapsedSecs: number;
  errorMessage: string | null;
  isMock: boolean;
}

// ─── Training ────────────────────────────────────────────────────────

export interface TrainingParams {
  algorithm: string;
  c1: number;
  c2: number;
  maxIterations: number;
  featurePossibleTransitions: boolean;
}

export const DEFAULT_TRAINING_PARAMS: TrainingParams = {
  algorithm: "lbfgs",
  c1: 1.0,
  c2: 1e-3,
  maxIterations: 100,
  featurePossibleTransitions: true,
};

export interface TrainRequest {
  corpusPaths: string[];
  featuresPath: string | null;
  modelPath: string | null;
  params: TrainingParams;
  includeDictionary: boolean;
  regenerateDictionaryFeatures: boolean;
}

export interface TrainingResult {
  success: boolean;
  modelPath: string | null;
  modelSizeBytes: number;
  trainingTimeSecs: number;
  sentenceCount: number;
  tokenCount: number;
  lastIteration: number | null;
  loss: number | null;
  featureCount: number | null;
  errorMessage: string | null;
  isMock: boolean;
}

// ─── Prediction ──────────────────────────────────────────────────────

export interface PredictRequest {
  inputText: string;
  modelPath: string | null;
  nBest: number;
  debug: boolean;
}

export interface FeatureValue {
  key: string;
  value: string;
  weight: number | null;
}

export interface TransitionScore {
  from: string;
  to: string;
  score: number;
}

export interface NBestCandidate {
  rank: number;
  score: number;
  labels: string[];
  bunsetsu: Bunsetsu[];
}

export interface PredictionResult {
  inputText: string;
  modelPath: string;
  tokens: string[];
  modelLabels: string[];
  /** `features[tokenIndex]` */
  features: FeatureValue[][];
  /** `emissionScores[tokenIndex][labelIndex]` */
  emissionScores: number[][];
  transitions: TransitionScore[];
  /** Per-gap confidence in `0..1`, length `tokens.length - 1`. */
  boundaryScores: number[];
  candidates: NBestCandidate[];
  debug: boolean;
  isMock: boolean;
}

// ─── Progress events ─────────────────────────────────────────────────

export type LogLevel = "info" | "warn" | "error" | "success";

export interface ProgressEvent {
  jobId: string;
  stage: string;
  message: string;
  current: number | null;
  total: number | null;
  level: LogLevel;
}

export interface LogLine {
  timestamp: string;
  jobId: string;
  stage: string;
  message: string;
  level: LogLevel;
}

// ─── UI-only types ───────────────────────────────────────────────────

export type ViewId = "test" | "train" | "stats";

export type ResultViewMode = "table" | "canvas";

/** Feature keys the Test tab grid renders, in display order. */
export const DISPLAY_FEATURE_KEYS = [
  "char",
  "char_left",
  "char_right",
  "bigram_left",
  "bigram_right",
  "dict_max_kl_s",
  "dict_max_kl_e",
  "dict_entry_ct_s",
  "dict_entry_ct_e",
] as const;

/** Human-readable labels for the feature rows. */
export const FEATURE_LABELS: Record<string, string> = {
  char: "char",
  char_left: "char[-1]",
  char_right: "char[+1]",
  bigram_left: "bigram[-1]",
  bigram_right: "bigram[+1]",
  dict_max_kl_s: "dict KL(start)",
  dict_max_kl_e: "dict KL(end)",
  dict_entry_ct_s: "dict count(start)",
  dict_entry_ct_e: "dict count(end)",
};
