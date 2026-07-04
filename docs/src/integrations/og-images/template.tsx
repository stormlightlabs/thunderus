import ImageResponse from "@takumi-rs/image-response";
import { readFile } from "node:fs/promises";

const colors = {
  bg: "#171928",
  panel: "#212337",
  panelHot: "#292e42",
  text: "#ebfafa",
  muted: "#abb4da",
  comment: "#7081d0",
  cyan: "#04d1f9",
  green: "#37f499",
  purple: "#a48cf2",
  pink: "#f265b5",
  yellow: "#f1fc79",
  orange: "#f7c67f",
  red: "#f16c75",
};

const fontBaseUrl = new URL("../../../node_modules/@fontsource-variable/ibm-plex-sans/files/", import.meta.url);
const displayFontBaseUrl = new URL("../../../node_modules/@fontsource-variable/literata/files/", import.meta.url);

const fonts = Promise.all([
  readFile(new URL("ibm-plex-sans-latin-wght-normal.woff2", fontBaseUrl)).then((data) => ({
    name: "IBM Plex Sans",
    data,
    weight: 400,
    style: "normal" as const,
  })),
  readFile(new URL("literata-latin-wght-normal.woff2", displayFontBaseUrl)).then((data) => ({
    name: "Literata",
    data,
    weight: 500,
    style: "normal" as const,
  })),
]);

function StatusPill({ label, value, color = colors.text }: { label: string; value: string; color?: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <span style={{ color: colors.comment }}>{label}</span>
      <span style={{ color, fontWeight: 700 }}>{value}</span>
    </div>
  );
}

function OgImage() {
  return (
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        position: "relative",
        overflow: "hidden",
        backgroundColor: colors.bg,
        color: colors.text,
        fontFamily: "IBM Plex Sans",
      }}>
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          background:
            "linear-gradient(135deg, rgba(4, 209, 249, 0.16), rgba(55, 244, 153, 0.06) 44%, rgba(242, 101, 181, 0.12))",
        }}
      />
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          opacity: 0.16,
          backgroundImage:
            "linear-gradient(rgba(235, 250, 250, 0.13) 1px, transparent 1px), linear-gradient(90deg, rgba(235, 250, 250, 0.11) 1px, transparent 1px)",
          backgroundSize: "40px 40px",
        }}
      />
      <div
        style={{
          position: "absolute",
          left: 76,
          top: 64,
          width: 1048,
          height: 500,
          display: "flex",
          flexDirection: "column",
          border: `2px solid ${colors.panelHot}`,
          borderRadius: 12,
          backgroundColor: colors.bg,
          overflow: "hidden",
          boxShadow: "0 32px 90px rgba(0, 0, 0, 0.38)",
        }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            height: 54,
            padding: "0 24px",
            borderBottom: `1px solid ${colors.panelHot}`,
            backgroundColor: colors.panel,
          }}>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <span style={{ width: 11, height: 11, borderRadius: 999, backgroundColor: colors.red }} />
            <span style={{ width: 11, height: 11, borderRadius: 999, backgroundColor: colors.yellow }} />
            <span style={{ width: 11, height: 11, borderRadius: 999, backgroundColor: colors.green }} />
            <span style={{ marginLeft: 10, color: colors.text, fontSize: 20, fontWeight: 700 }}>
              thndrs.stormlightlabs.org
            </span>
          </div>
        </div>
        <div
          style={{
            display: "flex",
            flex: 1,
            flexDirection: "column",
            justifyContent: "center",
            maxWidth: 820,
            padding: "32px 48px 24px",
            letterSpacing: 0,
            backgroundColor: colors.bg,
          }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              <div style={{ fontFamily: "Literata", color: colors.text, fontSize: 60, fontWeight: 500, lineHeight: 1 }}>
                thndrs
              </div>
            </div>
          </div>
          <div style={{ display: "flex", marginTop: 26 }}>
            <div
              style={{
                display: "flex",
                flex: 1,
                alignItems: "center",
                padding: "18px 22px",
                border: `1px solid ${colors.panelHot}`,
                backgroundColor: colors.panel,
              }}>
              <span style={{ color: colors.green, fontSize: 24, fontWeight: 700, marginRight: 16 }}>{">"}</span>
              <span style={{ color: colors.text, fontSize: 22 }}>A minimal, AI-powered pair programmer.</span>
            </div>
          </div>
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            height: 46,
            flexShrink: 0,
            padding: "0 24px",
            borderTop: `1px solid ${colors.panelHot}`,
            backgroundColor: colors.panel,
            fontSize: 16,
            color: colors.muted,
          }}>
          <span>github.com/stormlightlabs/thunderus</span>
        </div>
      </div>
    </div>
  );
}

export async function generateOGImage() {
  const response = new ImageResponse(<OgImage />, {
    width: 1200,
    height: 630,
    format: "png",
    fonts: await fonts,
    headers: { "Cache-Control": "public, max-age=31536000, immutable" },
  });

  await response.ready;
  return response;
}
