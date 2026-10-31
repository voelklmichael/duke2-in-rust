// Reads zip files with the browser's own inflater (DecompressionStream):
// stored and deflated entries, no encryption, no Zip64.

const END_OF_DIRECTORY = 0x06054b50;
const DIRECTORY_ENTRY = 0x02014b50;
const LOCAL_HEADER = 0x04034b50;

/**
 * The entries of the zip in `buffer` (an ArrayBuffer): `{ path, size, read }`
 * for each file, where `read()` resolves to its bytes (a Uint8Array).
 * Folders are left out.
 */
export function readZip(buffer) {
  const view = new DataView(buffer);
  const end = findEnd(view);
  const count = view.getUint16(end + 10, true);
  let offset = view.getUint32(end + 16, true);
  if (count === 0xffff || offset === 0xffffffff) throw new Error('Zip64 archives are not supported.');
  const entries = [];
  for (let i = 0; i < count; i++) {
    if (view.getUint32(offset, true) !== DIRECTORY_ENTRY) throw new Error('The zip file is damaged.');
    const flags = view.getUint16(offset + 8, true);
    const method = view.getUint16(offset + 10, true);
    const packed = view.getUint32(offset + 20, true);
    const size = view.getUint32(offset + 24, true);
    const nameLength = view.getUint16(offset + 28, true);
    const skip = nameLength + view.getUint16(offset + 30, true) + view.getUint16(offset + 32, true);
    const local = view.getUint32(offset + 42, true);
    const nameBytes = new Uint8Array(buffer, offset + 46, nameLength);
    // Without the UTF-8 flag names are in code page 437; the game's are ASCII.
    const path = new TextDecoder(flags & 0x800 ? 'utf-8' : 'latin1').decode(nameBytes).replace(/\\/g, '/');
    offset += 46 + skip;
    if (path.endsWith('/')) continue;
    entries.push({
      path,
      size,
      read: () => {
        if (flags & 1) throw new Error(`${path} is encrypted.`);
        if (view.getUint32(local, true) !== LOCAL_HEADER) throw new Error('The zip file is damaged.');
        const start = local + 30 + view.getUint16(local + 26, true) + view.getUint16(local + 28, true);
        return inflate(new Uint8Array(buffer, start, packed), method, path);
      },
    });
  }
  return entries;
}

/** The offset of the end of central directory record, after any comment. */
function findEnd(view) {
  const last = Math.max(0, view.byteLength - 22 - 0xffff);
  for (let at = view.byteLength - 22; at >= last; at--) {
    if (view.getUint32(at, true) === END_OF_DIRECTORY) return at;
  }
  throw new Error('This is not a zip file.');
}

async function inflate(bytes, method, path) {
  if (method === 0) return bytes.slice();
  if (method !== 8) throw new Error(`${path} uses an unsupported compression method (${method}).`);
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}
