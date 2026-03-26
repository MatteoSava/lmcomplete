import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  interpolate,
  spring,
} from "remotion";
import { Terminal } from "./Terminal";

export const Scene5Trust: React.FC = () => {
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
    { type: "comment" as const, content: "# Inspect what gets sent to the LLM", delay: 0.5 },
    { type: "input" as const, content: 'lmc audit \'curl -H "Authorization: Bearer sk-secret" https://api.example.com\'', delay: 1.5 },
    { type: "output" as const, content: "", delay: 3.5 },
    { type: "output" as const, content: "=== Prompt Bundle ===", delay: 4.0 },
    { type: "output" as const, content: "", delay: 4.3 },
    { type: "output" as const, content: "shell: zsh", delay: 4.6 },
    { type: "output" as const, content: "os: macos", delay: 4.9 },
    { type: "output" as const, content: "cwd: /Users/matteo/project", delay: 5.2 },
    { type: "output" as const, content: "", delay: 5.5 },
    { type: "output" as const, content: "command:", delay: 5.8 },
    { type: "output" as const, content: '  curl -H "Authorization: Bearer [REDACTED]" https://api.example.com', delay: 6.2 },
    { type: "comment" as const, content: "", delay: 7.5 },
    { type: "comment" as const, content: "# Secrets are redacted before sending", delay: 8.0 },
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
          <span style={{ color: "#f38ba8" }}>Trust</span> & Privacy
        </h1>
        <p
          style={{
            fontSize: 24,
            color: "#8b949e",
            margin: "8px 0 0 0",
          }}
        >
          Audit what leaves your machine
        </p>
      </div>

      {/* Terminal */}
      <Terminal title="Terminal — zsh" lines={lines} startFrame={15} />
    </AbsoluteFill>
  );
};
