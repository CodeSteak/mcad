import { deflateRawSync } from "node:zlib";
import { describe, expect, it } from "vitest";
import { crc32, readZip, writeZip } from "./zip";

/** ZIP with one deflated entry (as `zip` writes it). */
function deflatedZip(name: string, text: string): Uint8Array {
  const data = new TextEncoder().encode(text);
  const comp = deflateRawSync(data);
  const n = new TextEncoder().encode(name);
  const local = Buffer.alloc(30 + n.length);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(20, 4);
  local.writeUInt16LE(8, 8);
  local.writeUInt32LE(crc32(data), 14);
  local.writeUInt32LE(comp.length, 18);
  local.writeUInt32LE(data.length, 22);
  local.writeUInt16LE(n.length, 26);
  Buffer.from(n).copy(local, 30);
  const cd = Buffer.alloc(46 + n.length);
  cd.writeUInt32LE(0x02014b50, 0);
  cd.writeUInt16LE(8, 10);
  cd.writeUInt32LE(crc32(data), 16);
  cd.writeUInt32LE(comp.length, 20);
  cd.writeUInt32LE(data.length, 24);
  cd.writeUInt16LE(n.length, 28);
  cd.writeUInt32LE(0, 42);
  Buffer.from(n).copy(cd, 46);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(1, 8);
  end.writeUInt16LE(1, 10);
  end.writeUInt32LE(cd.length, 12);
  end.writeUInt32LE(local.length + comp.length, 16);
  return new Uint8Array(Buffer.concat([local, comp, cd, end]));
}

describe("ZIP", () => {
  it("CRC-32", () => {
    expect(crc32(new TextEncoder().encode("123456789"))).toBe(0xcbf43926);
  });

  it("write and read (stored, UTF-8 names)", async () => {
    const z = writeZip([
      { name: "project/main.scad", data: "include <naïve.scad>\ncube(1);\n" },
      { name: "project/naïve.scad", data: "b = 1; // é ü" },
    ]);
    const back = await readZip(z);
    const dec = new TextDecoder();
    expect(back.map((e) => [e.name, dec.decode(e.data)])).toEqual([
      ["project/main.scad", "include <naïve.scad>\ncube(1);\n"],
      ["project/naïve.scad", "b = 1; // é ü"],
    ]);
  });

  it("compressed entries (deflate)", async () => {
    const text = "module m() cube(1);\n".repeat(50);
    const back = await readZip(deflatedZip("lib/parts.mcad", text));
    expect(back[0].name).toBe("lib/parts.mcad");
    expect(new TextDecoder().decode(back[0].data)).toBe(text);
  });

  it("not a ZIP file", async () => {
    await expect(readZip(new TextEncoder().encode("cube(1);"))).rejects.toThrow("keine ZIP-Datei");
  });
});
