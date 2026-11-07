import {
  POCKET_CAPABILITIES,
  definePlatformContractRegistry,
  defineTargetRegistry,
} from "../../references/pocketjs/contracts/spec/platforms.ts";
import type { ResolvedBuildPlan } from "../../references/pocketjs/framework/src/manifest/plan.ts";
import { validateAndResolveBuildPlan } from "../../references/pocketjs/framework/src/manifest/resolve.ts";

/**
 * X-T4 rear LCD under the bare-metal fuji-boost firmware. Like PocketJS's
 * own `iphone2g-dev`, the profile stays outside the production
 * `POCKET_TARGETS` registry: the firmware owns the whole 900x600 panel and
 * renders a 450x300 logical surface that LCDC1 scales 2x. Input is the six
 * navigation keys (d-pad, MENU/OK as CIRCLE, DISP/BACK as CROSS) and the
 * two-finger rear touch panel, sampled once per frame.
 */
export const FUJI_XT4_TARGET_ID = "fuji-xt4";
export const FUJI_XT4_HOST_ABI = 40;
export const FUJI_XT4_VIEWPORT = [450, 300] as const;

export const FUJI_XT4_CONTRACTS = definePlatformContractRegistry(
  POCKET_CAPABILITIES,
  defineTargetRegistry({
    [FUJI_XT4_TARGET_ID]: {
      hostAbi: FUJI_XT4_HOST_ABI,
      platform: "fuji-xp4",
      form: "takeover",
      display: {
        physicalViewport: FUJI_XT4_VIEWPORT,
        logicalViewports: [FUJI_XT4_VIEWPORT],
        presentations: ["native"],
        rasterDensity: 1,
      },
      capabilities: ["input.buttons", "input.touch", "text.glyphs.baked"],
    },
  }),
);

export function resolveFujiBuildPlan(manifest: unknown): ResolvedBuildPlan {
  const resolution = validateAndResolveBuildPlan(
    manifest,
    { target: FUJI_XT4_TARGET_ID },
    FUJI_XT4_CONTRACTS,
  );
  if (!resolution.ok) {
    throw new Error(
      `fuji-xt4: manifest did not resolve: ${resolution.diagnostics
        .map((diagnostic) => `${diagnostic.path || "/"}: ${diagnostic.message}`)
        .join("; ")}`,
    );
  }
  return resolution.plan;
}
