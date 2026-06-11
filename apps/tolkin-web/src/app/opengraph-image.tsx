import { ImageResponse } from "next/og";

export const alt = "Tolkin: the privacy-first AI token analyzer";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

// Purely typographic card: wordmark, one measured number, the URL.
export default function OpengraphImage() {
  return new ImageResponse(
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        backgroundColor: "#0c0c0b",
        padding: "72px 80px",
        fontFamily: "sans-serif",
      }}
    >
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 28,
        }}
      >
        <div
          style={{
            fontSize: 132,
            fontWeight: 800,
            color: "#f3f2ee",
            letterSpacing: "-0.05em",
            lineHeight: 1,
          }}
        >
          tolkin
        </div>
        <div
          style={{
            fontSize: 34,
            color: "#9b9a93",
            letterSpacing: "-0.01em",
            maxWidth: 880,
            lineHeight: 1.35,
          }}
        >
          The privacy-first token analyzer for AI agent setups.
        </div>
      </div>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "flex-end",
        }}
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          <div
            style={{
              fontSize: 64,
              fontWeight: 700,
              color: "#bef264",
              letterSpacing: "-0.02em",
            }}
          >
            0.97522 measured
          </div>
          <div style={{ fontSize: 24, color: "#73726b" }}>
            cache hit rate, on real session logs
          </div>
        </div>
        <div style={{ fontSize: 28, color: "#9b9a93" }}>tolkin.dev</div>
      </div>
    </div>,
    { ...size },
  );
}
