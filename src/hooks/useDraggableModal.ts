import { useState, useEffect, useRef } from "react";

export interface UseDraggableModalOptions {
  isOpen: boolean;
  initialX?: number;
  initialY?: number;
  boundaryMarginX?: number;
  boundaryMarginY?: number;
}

export function useDraggableModal({
  isOpen,
  initialX = 0,
  initialY = 0,
  boundaryMarginX = 140,
  boundaryMarginY = 100,
}: UseDraggableModalOptions) {
  const [position, setPosition] = useState<{ x: number; y: number }>({
    x: initialX,
    y: initialY,
  });
  const [isDragging, setIsDragging] = useState(false);
  const dragStartRef = useRef<{
    startX: number;
    startY: number;
    posX: number;
    posY: number;
  }>({
    startX: 0,
    startY: 0,
    posX: 0,
    posY: 0,
  });

  // Reset to default on fresh open
  useEffect(() => {
    if (isOpen) {
      setPosition({ x: initialX, y: initialY });
    }
  }, [isOpen, initialX, initialY]);

  // Global mousemove and mouseup listeners
  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      const deltaX = e.clientX - dragStartRef.current.startX;
      const deltaY = e.clientY - dragStartRef.current.startY;
      const newX = dragStartRef.current.posX + deltaX;
      const newY = dragStartRef.current.posY + deltaY;

      // Safe viewport boundary clamping so the modal cannot be dragged completely off-screen
      const maxOffsetX = Math.max(0, window.innerWidth / 2 - boundaryMarginX);
      const maxOffsetY = Math.max(0, window.innerHeight / 2 - boundaryMarginY);

      setPosition({
        x: Math.max(-maxOffsetX, Math.min(maxOffsetX, newX)),
        y: Math.max(-maxOffsetY, Math.min(maxOffsetY, newY)),
      });
    };

    const handleMouseUp = () => {
      setIsDragging(false);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDragging, boundaryMarginX, boundaryMarginY]);

  const handleMouseDown = (e: React.MouseEvent) => {
    // Ignore clicks on buttons, inputs, selects, or interactive elements
    const target = e.target as HTMLElement;
    if (
      target.closest("button") ||
      target.closest("input") ||
      target.closest("select") ||
      target.closest("a") ||
      target.closest("[data-no-drag]")
    ) {
      return;
    }

    setIsDragging(true);
    dragStartRef.current = {
      startX: e.clientX,
      startY: e.clientY,
      posX: position.x,
      posY: position.y,
    };
  };

  const resetPosition = () => {
    setPosition({ x: 0, y: 0 });
  };

  const isDisplaced = position.x !== 0 || position.y !== 0;

  return {
    position,
    isDragging,
    handleMouseDown,
    resetPosition,
    isDisplaced,
  };
}
