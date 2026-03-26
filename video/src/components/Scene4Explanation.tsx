import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  interpolate,
  spring,
} from "remotion";
import { Terminal } from "./Terminal";

export const Scene4Explanation: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const titleSpring = spring({
    frame,
    fps,
    config: { damping: 200 },
  });

  const titleY = interpolate(titleSpring, [0, 1], [-50, 0]);
  const titleOpacity = titleSpring;

  const lines = [
    { type: "comment" as const, content: "# See what a command does", delay: 0.5 },
    { type: "input" as const, content: 'lmc explain "tar xzf archive.tar.gz"', delay: 1.5 },
    { type: "output" as const, content: "", delay: 3.0 },
    { type: "output" as const, content: "Extracts a gzip-compressed tar archive.", delay: 3.5 },
    { type: "output" as const, content: "", delay: 4.0 },
    { type: "output" as const, content: "  tar   — tape archive utility", delay: 4.3 },
    { type: "output" as const, content: "  x     — extract files", delay: 4.6 },
    { type: "output" as const, content: "  z     — decompress with gzip", delay: 4.9 },
    { type: "output" as const, content: "  f     — read from file", delay: 5.2 },
    { type: "comment" as const, content: "", delay: 6.5 },
    { type: "comment" as const, content: "# Concise. Readable. One glance.", delay: 7.0 },
  ];

  return (
    <AbsoluteFill
      style={{
        backgroundColor: "#0d1117",
        flexDirection: "column",
      }}
    >
      {/* Title */}
      <div
        style={{
          position: "absolute",
          top: 40,
          left: 0,
          right: 0,
          textAlign: "center",
          transform: `translateY(${titleY}px)`,
          opacity: titleOpacity,
        }}
      >
        <h1
          style={{
            fontSize: 48,
            fontWeight: 700,
            color: "#ffffff",
            margin: 0,
            fontFamily: "system-ui, sans-serif",
          }}
        >
          Clear <span style={{ color: "#cba6f7" }}>Explanations</span>
        </h1>
        <p
          style={{
            fontSize: 24,
            color: "#8b949e",
            margin: "8px 0 0 0",
          }}
        >
          Understand before you run
        </p>
      </div>

      {/* Terminal */}
      <Terminal title="Terminal — zsh" lines={lines} startFrame={15} />
    </AbsoluteFill>
  );
};
