import { useState, useEffect, useRef } from "react";

export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const safeI = Math.min(i, units.length - 1);
  return `${(bytes / Math.pow(1024, safeI)).toFixed(2)} ${units[safeI]}`;
}

export function formatEtaDuration(seconds: number): string {
  if (seconds <= 0) return "ETA: Finishing...";
  if (seconds < 60) return `ETA: ${seconds}s`;
  if (seconds < 3600) {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `ETA: ${m}m ${s}s`;
  }
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `ETA: ${h}h ${m}m`;
}

interface DynamicThroughputOptions {
  isActive: boolean;
  bytesProcessed: number;
  totalBytes: number;
}

interface DynamicThroughputResult {
  speedText: string | null;
  etaText: string | null;
  speedBytesPerSec: number;
  remainingSeconds: number | null;
}

interface Sample {
  time: number;
  bytes: number;
}

/**
 * High-precision dynamic throughput and real-time countdown ETA hook.
 *
 * Solves the static ETA / stuck progress problem:
 * 1. Independent 200ms tick interval ensures the UI updates smoothly even when I/O pauses or stalls.
 * 2. Sliding window (2.0s) computes real-time instantaneous speed instead of a misleading lifetime average.
 * 3. Exponential moving average (EMA) smooths out spikes and gently decays during disk buffer flushes.
 * 4. Real-time target completion countdown ticks second-by-second (3s -> 2s -> 1s -> Finishing...).
 */
export function useDynamicThroughput({
  isActive,
  bytesProcessed,
  totalBytes,
}: DynamicThroughputOptions): DynamicThroughputResult {
  const [, setTick] = useState(0);

  const startTimeRef = useRef<number | null>(null);
  const samplesRef = useRef<Sample[]>([]);
  const smoothedSpeedRef = useRef<number>(0);
  const lastBytesRef = useRef<number>(0);
  const lastBytesTimeRef = useRef<number>(0);
  const targetCompletionTimeRef = useRef<number | null>(null);
  const lastEtaSecRef = useRef<number | null>(null);

  // Active 200ms tick timer runs while the operation is active
  useEffect(() => {
    if (!isActive) {
      startTimeRef.current = null;
      samplesRef.current = [];
      smoothedSpeedRef.current = 0;
      lastBytesRef.current = 0;
      lastBytesTimeRef.current = 0;
      targetCompletionTimeRef.current = null;
      lastEtaSecRef.current = null;
      return;
    }

    if (!startTimeRef.current) {
      const now = Date.now();
      startTimeRef.current = now;
      lastBytesTimeRef.current = now;
      lastBytesRef.current = bytesProcessed;
      samplesRef.current = [{ time: now, bytes: bytesProcessed }];
    }

    const timer = setInterval(() => {
      setTick((t) => (t + 1) % 1000000);
    }, 200);

    return () => clearInterval(timer);
  }, [isActive]);

  // Track progress samples on byte changes
  useEffect(() => {
    if (!isActive) return;
    const now = Date.now();

    if (bytesProcessed > lastBytesRef.current) {
      lastBytesRef.current = bytesProcessed;
      lastBytesTimeRef.current = now;
      samplesRef.current.push({ time: now, bytes: bytesProcessed });

      // Keep sliding window within last 2.0 seconds
      const cutoff = now - 2000;
      samplesRef.current = samplesRef.current.filter((s) => s.time >= cutoff);
    }
  }, [isActive, bytesProcessed]);

  if (!isActive || bytesProcessed <= 0 || !startTimeRef.current) {
    return {
      speedText: null,
      etaText: null,
      speedBytesPerSec: 0,
      remainingSeconds: null,
    };
  }

  const now = Date.now();
  const totalElapsed = (now - startTimeRef.current) / 1000;
  if (totalElapsed < 0.4) {
    return {
      speedText: null,
      etaText: null,
      speedBytesPerSec: 0,
      remainingSeconds: null,
    };
  }

  const samples = samplesRef.current;
  let instantSpeed = 0;

  if (samples.length >= 2) {
    const oldest = samples[0];
    const newest = samples[samples.length - 1];
    const dt = (newest.time - oldest.time) / 1000;
    const db = newest.bytes - oldest.bytes;
    if (dt >= 0.25 && db > 0) {
      instantSpeed = db / dt;
    }
  }

  // Update EMA smoothed speed
  if (instantSpeed > 0) {
    if (smoothedSpeedRef.current <= 0) {
      smoothedSpeedRef.current = instantSpeed;
    } else {
      smoothedSpeedRef.current = smoothedSpeedRef.current * 0.65 + instantSpeed * 0.35;
    }
  } else if (smoothedSpeedRef.current <= 0 && totalElapsed >= 0.5) {
    smoothedSpeedRef.current = bytesProcessed / totalElapsed;
  }

  // Smooth decay if no new bytes arrived in > 500ms (e.g. disk write flush or worker thread join)
  const idleMs = now - lastBytesTimeRef.current;
  let activeSpeed = smoothedSpeedRef.current;
  if (idleMs > 500) {
    const idleSec = idleMs / 1000;
    const decay = Math.max(0.08, 1 - (idleSec - 0.5) * 0.35);
    activeSpeed = smoothedSpeedRef.current * decay;
  }

  const speedFormatted = activeSpeed > 0 ? `${formatBytes(activeSpeed)}/s` : null;

  // Remaining bytes and dynamic countdown ETA
  const remainingBytes = Math.max(0, totalBytes - bytesProcessed);
  let etaFormatted: string | null = null;
  let remainingSeconds: number | null = null;

  if (remainingBytes <= 0) {
    etaFormatted = "ETA: Finishing...";
    remainingSeconds = 0;
  } else if (activeSpeed > 0) {
    const rawEtaSec = Math.max(1, Math.round(remainingBytes / activeSpeed));

    if (idleMs <= 500) {
      // Progress actively flowing: adapt target completion time smoothly
      const smoothEta =
        lastEtaSecRef.current === null
          ? rawEtaSec
          : Math.round(lastEtaSecRef.current * 0.6 + rawEtaSec * 0.4);
      targetCompletionTimeRef.current = now + smoothEta * 1000;
      lastEtaSecRef.current = smoothEta;
      remainingSeconds = smoothEta;
    } else {
      // Progress stalled (e.g. disk flush or final worker sync): actively count down
      if (targetCompletionTimeRef.current) {
        const msLeft = targetCompletionTimeRef.current - now;
        remainingSeconds = Math.round(msLeft / 1000);
      } else {
        remainingSeconds = rawEtaSec;
      }
    }

    if (remainingSeconds <= 0) {
      etaFormatted = "ETA: Finishing...";
    } else if (remainingSeconds <= 1 && totalBytes > 0 && bytesProcessed / totalBytes >= 0.7) {
      etaFormatted = "ETA: Finishing...";
    } else {
      etaFormatted = formatEtaDuration(remainingSeconds);
    }
  } else {
    etaFormatted = "ETA: Finishing...";
  }

  return {
    speedText: speedFormatted,
    etaText: etaFormatted,
    speedBytesPerSec: activeSpeed,
    remainingSeconds,
  };
}
