export type TitleProvenance =
  | "operator_override"
  | "canonical_remote"
  | "job_snapshot"
  | "imported_file"
  | "stable_provider_id";

export type CanonicalTitleProjection = {
  target_title?: string | null;
  target_title_provenance?: TitleProvenance | null;
  target_title_problem?: string | null;
};

export type CanonicalLibraryTitleProjection = {
  title: string;
  title_provenance?: TitleProvenance | null;
  title_problem?: string | null;
};

export function titleProvenanceLabel(
  value: TitleProvenance | string | null | undefined,
): string | null {
  switch (value) {
    case "operator_override":
      return "Operator title";
    case "canonical_remote":
      return "Canonical provider title";
    case "job_snapshot":
      return "Enqueue-time provider title";
    case "imported_file":
      return "Imported or file title";
    case "stable_provider_id":
      return "Provider ID fallback";
    default:
      return null;
  }
}
