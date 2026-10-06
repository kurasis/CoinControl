import type { QueryClient } from "@tanstack/react-query";

/** Preserve a user's pending next page, then refresh its completed page set. */
export async function refreshCachedViews(client: QueryClient) {
  const pending = client
    .getQueryCache()
    .findAll({ type: "active", fetchStatus: "fetching" }).length;
  await client.invalidateQueries({}, { cancelRefetch: false });
  if (pending) await client.refetchQueries({ type: "active" }, { cancelRefetch: false });
}
