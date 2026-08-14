export type ProjectionRequestIdentity = {
  generation: number;
  queryKey: string;
};

export function isProjectionRequestCurrent(
  request: ProjectionRequestIdentity,
  current: ProjectionRequestIdentity,
): boolean {
  return request.generation === current.generation && request.queryKey === current.queryKey;
}
