export const PLATFORMS = ["twitch", "youtube", "kick", "facebook"] as const;
export type Platform = (typeof PLATFORMS)[number];

export type PlatformStatus =
  | "idle"
  | "connecting"
  | "live"
  | "reconnecting"
  | "error";

export interface PlatformConfig {
  enabled: boolean;
  streamKey: string;
  serverUrl: string;
}

export interface AppConfig {
  ingestPort: number;
  platforms: Record<Platform, PlatformConfig>;
}

export interface IngestStatus {
  status: "offline" | "waiting" | "receiving";
  bitrateKbps?: number | null;
}

export interface PlatformMetrics {
  platform: Platform;
  bitrateKbps: number;
  speed: number;
  fps: number;
  uptimeSec: number;
  reconnects: number;
}

export interface SystemMetrics {
  cpuPercent: number;
  memUsedMb: number;
  memTotalMb: number;
}

export interface LogEntry {
  ts: number;
  level: string;
  source: string;
  message: string;
}
