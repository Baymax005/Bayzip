import { useState } from "react";

interface BayzipLogoProps {
  className?: string;
  fallbackClassName?: string;
  alt?: string;
}

/**
 * Resilient Bayzip Logo component.
 * Attempts to load the official PNG asset from `/bayzip-logo.png`.
 * If it encounters any loading or path error, it seamlessly renders
 * an authentic SVG Fluent archiver emblem so the brand icon never disappears.
 */
export function BayzipLogo({
  className = "w-5 h-5",
  fallbackClassName = "w-5 h-5",
  alt = "Bayzip",
}: BayzipLogoProps) {
  const [loadFailed, setLoadFailed] = useState(false);

  if (loadFailed) {
    return (
      <svg
        className={`${fallbackClassName} text-[#60CDFF] shrink-0`}
        viewBox="0 0 24 24"
        fill="currentColor"
        aria-label={alt}
      >
        <path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm-7 2h2v2h-2V5zm0 4h2v2h-2V9zm0 4h2v2h-2v-2zm-4 4v-2h8v2H8z" />
      </svg>
    );
  }

  return (
    <img
      src="/bayzip-logo.png"
      alt={alt}
      className={`${className} object-contain shrink-0`}
      onError={() => setLoadFailed(true)}
    />
  );
}
