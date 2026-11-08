import React, { useRef, useState, useEffect } from "react";

export interface VirtualScrollListProps<T> {
  items: T[];
  rowHeight?: number;
  overscan?: number;
  className?: string;
  renderRow: (item: T, index: number) => React.ReactNode;
}

export function VirtualScrollList<T>({
  items,
  rowHeight = 34,
  overscan = 5,
  className = "",
  renderRow,
}: VirtualScrollListProps<T>) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(300);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;

    let rafId: number | null = null;
    const onScroll = () => {
      if (rafId !== null) return;
      rafId = requestAnimationFrame(() => {
        if (el) {
          setScrollTop(el.scrollTop);
        }
        rafId = null;
      });
    };

    const updateHeight = () => {
      if (el) {
        setViewportHeight(el.clientHeight || 300);
      }
    };

    updateHeight();
    el.addEventListener("scroll", onScroll, { passive: true });

    let ro: ResizeObserver | null = null;
    if (typeof ResizeObserver !== "undefined") {
      ro = new ResizeObserver(() => updateHeight());
      ro.observe(el);
    } else {
      window.addEventListener("resize", updateHeight);
    }

    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      el.removeEventListener("scroll", onScroll);
      if (ro) ro.disconnect();
      else window.removeEventListener("resize", updateHeight);
    };
  }, []);

  const totalHeight = items.length * rowHeight;
  const startIndex = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const endIndex = Math.min(items.length, Math.ceil((scrollTop + viewportHeight) / rowHeight) + overscan);
  const visibleItems = items.slice(startIndex, endIndex);
  const offsetY = startIndex * rowHeight;

  return (
    <div
      ref={scrollRef}
      onDragOver={(e) => {
        e.preventDefault();
      }}
      className={`overflow-y-auto custom-scrollbar relative ${className}`}
    >
      <div style={{ height: `${totalHeight}px`, position: "relative", width: "100%" }}>
        <div
          style={{
            transform: `translateY(${offsetY}px)`,
            position: "absolute",
            top: 0,
            left: 0,
            right: 0,
          }}
          className="divide-y divide-[#262626]"
        >
          {visibleItems.map((item, idx) => renderRow(item, startIndex + idx))}
        </div>
      </div>
    </div>
  );
}
