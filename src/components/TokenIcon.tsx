import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../ipc/client";
import { useFirstUsefulPaint } from "../app/useFirstUsefulPaint";

/** Optional backend-sanitized raster; never render provider URLs or HTML. */
export function TokenIcon({ symbol, assetId }: { symbol: string | null; assetId?: string }) {
  const ready = useFirstUsefulPaint(true);
  const [failedImage, setFailedImage] = useState<string | null>(null);
  const icon = useQuery({
    queryKey: ["asset-icon", assetId],
    queryFn: () => api.assetIcon(assetId!),
    enabled: ready && !!assetId,
    staleTime: 60 * 60 * 1000,
    retry: false,
  });
  const image =
    icon.data?.startsWith("data:image/png;base64,") && icon.data !== failedImage ? icon.data : null;
  const text =
    (symbol ?? "?")
      .replace(/[^\p{L}\p{N}]/gu, "")
      .slice(0, 3)
      .toUpperCase() || "?";
  return (
    <span className="token-icon" aria-hidden="true">
      {image ? <img src={image} alt="" onError={() => setFailedImage(image)} /> : text}
    </span>
  );
}
