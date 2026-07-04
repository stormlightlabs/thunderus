import { generateOGImage } from "./template";

export const prerender = true;

export function GET() {
  return generateOGImage();
}
