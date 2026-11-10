import type { Platform } from "./types";

export const PLATFORM_META: Record<
  Platform,
  { label: string; color: string; keyHint: string }
> = {
  twitch: {
    label: "Twitch",
    color: "#9146FF",
    keyHint: "dashboard.twitch.tv → Settings → Stream",
  },
  youtube: {
    label: "YouTube",
    color: "#FF0033",
    keyHint: "studio.youtube.com → Go Live",
  },
  kick: {
    label: "Kick",
    color: "#53FC18",
    keyHint: "kick.com → Creator Dashboard → Stream Key (URL is per-account)",
  },
  facebook: {
    label: "Facebook",
    color: "#0866FF",
    keyHint: "facebook.com/live/producer",
  },
};
