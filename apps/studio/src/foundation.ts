/**
 * GVYA Studio foundation marker.
 *
 * Studio is a human-facing editor over canonical GVYA source. Machine authors operate the same source
 * through external tools and the canonical CLI. Studio owns no model/provider execution, mechanic
 * classification, or retry orchestration.
 */

export interface StudioFoundation {
  readonly product: "GVYA Studio";
  readonly humanUxContractProtected: true;
  readonly providerNeutral: true;
}

export const studioFoundation: StudioFoundation = {
  product: "GVYA Studio",
  humanUxContractProtected: true,
  providerNeutral: true
};
