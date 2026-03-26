import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  interpolate,
  spring,
} from "remotion";

type TerminalLine = {
  type: "input" | "output" | "comment";
  content: string;
  delay?: number;
};

type TerminalProps = {
  title?: string;
  lines: TerminalLine[];
  startFrame?: number;
};

export const Terminal: React.FC<TerminalProps> = ({
  title = "Terminal",
  lines,
  startFrame = 0,
}) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const containerSpring = spring({
    frame: frame - startFrame,
    fps,
    config: { damping: 200 },
  });

  const scale = interpolate(containerSpring, [0, 1], [0.95, 1]);
  const opacity = interpolate(containerSpring, [0, 1], [0, 1]);

  return (
    <AbsoluteFill
      style={{
        justifyContent: "center",
        alignItems: "center",
        padding: 60,
      }}
    >
      <div
        style={{
          width: "100%",
          maxWidth: 1400,
          height: "auto",
          maxHeight: 800,
          backgroundColor: "#1e1e2e",
          borderRadius: 12,
          overflow: "hidden",
          boxShadow: "0 25px 80px rgba(0,0,0,0.5)",
          transform: `scale(${scale})`,
          opacity,
          fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
        }}
      >
        {/* Title bar */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            padding: "12px 16px",
            backgroundColor: "#2d2d3f",
            borderBottom: "1px solid #3d3d4f",
          }}
        >
          <div style={{ display: "flex", gap: 8 }}>
            <div
              style={{
                width: 12,
                height: 12,
                borderRadius: "50%",
                backgroundColor: "#ff5f57",
              }}
            />
            <div
              style={{
                width: 12,
                height: 12,
                borderRadius: "50%",
                backgroundColor: "#febc2e",
              }}
            />
            <div
              style={{
                width: 12,
                height: 12,
                borderRadius: "50%",
                backgroundColor: "#28c840",
              }}
            />
          </div>
          <div
            style={{
              flex: 1,
              textAlign: "center",
              color: "#8b8b9b",
              fontSize: 14,
              fontWeight: 500,
            }}
          >
            {title}
          </div>
        </div>

        {/* Terminal content */}
        <div
          style={{
            padding: 24,
            fontSize: 20,
            lineHeight: 1.6,
            color: "#cdd6f4",
          }}
        >
          {lines.map((line, index) => {
            const lineDelay = line.delay ?? index * 0.8;
            const lineFrame = frame - startFrame - lineDelay * fps;

            const lineProgress = spring({
              frame: lineFrame,
              fps,
              config: { damping: 200 },
            });

            const lineOpacity = interpolate(lineProgress, [0, 1], [0, 1]);

            if (lineProgress <= 0) return null;

            const isInput = line.type === "input";
            const isComment = line.type === "comment";

            return (
              <div
                key={index}
                style={{
                  opacity: lineOpacity,
                  marginBottom: 8,
                  color: isComment ? "#6c7086" : isInput ? "#89b4fa" : "#a6e3a1",
                }}
              >
                {isInput && (
                  <span style={{ color: "#f38ba8", marginRight: 8 }}>❯</span>
                )}
                <TypewriterText
                  text={line.content}
                  frame={lineFrame}
                  fps={fps}
                  speed={0.03}
                />
              </div>
            );
          })}
        </div>
      </div>
    </AbsoluteFill>
  );
};

type TypewriterTextProps = {
  text: string;
  frame: number;
  fps: number;
  speed?: number;
};

const TypewriterText: React.FC<TypewriterTextProps> = ({
  text,
  frame,
  fps,
  speed = 0.03,
}) => {
  // Handle empty text to avoid invalid input range [0, 0]
  if (text.length === 0) {
    return null;
  }

  const charsToShow = Math.floor(
    interpolate(frame, [0, text.length * speed * fps], [0, text.length], {
      extrapolateRight: "clamp",
      extrapolateLeft: "clamp",
    })
  );

  const displayText = text.slice(0, charsToShow);
  const showCursor = frame >= 0 && charsToShow < text.length;

  return (
    <span>
      {displayText}
      {showCursor && (
        <span
          style={{
            borderLeft: "2px solid #f38ba8",
            marginLeft: 1,
            animation: "blink 1s infinite",
          }}
        >
          &nbsp;
        </span>
      )}
    </span>
  );
};
