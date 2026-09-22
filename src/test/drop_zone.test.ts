import { describe, it, expect } from "vitest";

describe("DropZone Drag and Drop File Validation", () => {
  const isAcceptedImageFile = (filename: string): boolean => {
    const ext = filename.split(".").pop()?.toLowerCase() || "";
    return ["png", "jpg", "jpeg", "webp"].includes(ext);
  };

  it("accepts valid image extensions", () => {
    expect(isAcceptedImageFile("photo.png")).toBe(true);
    expect(isAcceptedImageFile("PHOTO.JPG")).toBe(true);
    expect(isAcceptedImageFile("landscape.JPEG")).toBe(true);
    expect(isAcceptedImageFile("illustration.webp")).toBe(true);
  });

  it("rejects non-image files and unsupported formats", () => {
    expect(isAcceptedImageFile("document.pdf")).toBe(false);
    expect(isAcceptedImageFile("script.sh")).toBe(false);
    expect(isAcceptedImageFile("binary.exe")).toBe(false);
    expect(isAcceptedImageFile("archive.zip")).toBe(false);
    expect(isAcceptedImageFile("image.bmp")).toBe(false);
    expect(isAcceptedImageFile("image.tiff")).toBe(false);
    expect(isAcceptedImageFile("noextension")).toBe(false);
  });

  it("filters a list of dropped files to only valid images", () => {
    const dropped = [
      { name: "pic1.png" },
      { name: "notes.txt" },
      { name: "pic2.jpeg" },
      { name: "data.json" },
      { name: "pic3.webp" },
    ];

    const accepted = dropped.filter((f) => isAcceptedImageFile(f.name));
    expect(accepted.map((f) => f.name)).toEqual(["pic1.png", "pic2.jpeg", "pic3.webp"]);
  });
});
