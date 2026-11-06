import { useEffect, useCallback } from "react";

interface UseModalDismissOptions {
  isOpen: boolean;
  onClose: () => void;
  disabled?: boolean;
}

/**
 * Standard hook for modal Escape key listener and backdrop click dismissal.
 * Prevents accidental dismissal during active operations (disabled: isProcessing).
 */
export function useModalDismiss({ isOpen, onClose, disabled = false }: UseModalDismissOptions) {
  useEffect(() => {
    if (!isOpen || disabled) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        e.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose, disabled]);

  const handleBackdropClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget && !disabled) {
        onClose();
      }
    },
    [onClose, disabled]
  );

  return { handleBackdropClick };
}
