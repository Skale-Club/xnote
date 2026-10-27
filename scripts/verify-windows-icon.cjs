#!/usr/bin/env node
'use strict';
// Read PE resources as data only. Never loads or executes the application.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const sha = (buffer) => crypto.createHash('sha256').update(buffer).digest('hex');

function icoFrames(bytes) {
  if (bytes.readUInt16LE(0) !== 0 || bytes.readUInt16LE(2) !== 1) throw new Error('Not an ICO file');
  const count = bytes.readUInt16LE(4), frames = [];
  for (let i = 0; i < count; i++) {
    const at = 6 + i * 16, size = bytes.readUInt32LE(at + 8), offset = bytes.readUInt32LE(at + 12);
    if (offset + size > bytes.length) throw new Error('Truncated ICO frame');
    frames.push({ width: bytes[at] || 256, height: bytes[at + 1] || 256, bits: bytes.readUInt16LE(at + 6), size, sha256: sha(bytes.subarray(offset, offset + size)) });
  }
  return frames;
}

function peResources(bytes) {
  if (bytes.toString('ascii', 0, 2) !== 'MZ') throw new Error('Not a Windows executable');
  const pe = bytes.readUInt32LE(0x3c);
  if (bytes.readUInt32LE(pe) !== 0x4550) throw new Error('Invalid PE signature');
  const optional = pe + 24, magic = bytes.readUInt16LE(optional);
  if (![0x10b, 0x20b].includes(magic)) throw new Error('Unknown PE format');
  const resourceRva = bytes.readUInt32LE(optional + (magic === 0x20b ? 112 : 96) + 16);
  if (!resourceRva) throw new Error('Executable has no resource directory');
  const sections = [], sectionTable = optional + bytes.readUInt16LE(pe + 20);
  for (let i = 0; i < bytes.readUInt16LE(pe + 6); i++) {
    const at = sectionTable + i * 40;
    sections.push({ rva: bytes.readUInt32LE(at + 12), length: Math.max(bytes.readUInt32LE(at + 8), bytes.readUInt32LE(at + 16)), raw: bytes.readUInt32LE(at + 20) });
  }
  function address(rva) {
    const section = sections.find((s) => rva >= s.rva && rva < s.rva + s.length);
    if (!section) throw new Error(`Resource RVA outside sections: ${rva}`);
    return section.raw + rva - section.rva;
  }
  const root = address(resourceRva), resources = [];
  function walk(relative, keys, depth) {
    if (depth > 8) throw new Error('Resource directory recursion limit');
    const dir = root + relative, count = bytes.readUInt16LE(dir + 12) + bytes.readUInt16LE(dir + 14);
    if (count > 10000) throw new Error('Invalid resource count');
    for (let i = 0; i < count; i++) {
      const at = dir + 16 + i * 8, name = bytes.readUInt32LE(at), offset = bytes.readUInt32LE(at + 4);
      const nameAt = root + (name & 0x7fffffff);
      const key = name & 0x80000000 ? bytes.toString('utf16le', nameAt + 2, nameAt + 2 + bytes.readUInt16LE(nameAt) * 2) : name;
      if (offset & 0x80000000) walk(offset & 0x7fffffff, [...keys, key], depth + 1);
      else {
        const data = root + offset, raw = address(bytes.readUInt32LE(data)), size = bytes.readUInt32LE(data + 4);
        if (raw + size > bytes.length) throw new Error('Truncated resource');
        resources.push({ keys: [...keys, key], data: bytes.subarray(raw, raw + size) });
      }
    }
  }
  walk(0, [], 0);
  return resources;
}

function inspectIcon(executable, icon) {
  const exe = fs.readFileSync(executable), expected = icoFrames(fs.readFileSync(icon)), resources = peResources(exe);
  const groups = resources.filter((r) => r.keys[0] === 14).map((group) => {
    const frames = [];
    if (group.data.readUInt16LE(2) !== 1) throw new Error('Invalid RT_GROUP_ICON');
    for (let i = 0; i < group.data.readUInt16LE(4); i++) {
      const at = 6 + i * 14, id = group.data.readUInt16LE(at + 12);
      const resource = resources.find((r) => r.keys[0] === 3 && r.keys[1] === id && r.keys[2] === group.keys[2]);
      if (!resource) throw new Error(`Missing RT_ICON ${id}`);
      frames.push({ width: group.data[at] || 256, height: group.data[at + 1] || 256, bits: group.data.readUInt16LE(at + 6), size: resource.data.length, sha256: sha(resource.data) });
    }
    return { id: group.keys[1], language: group.keys[2], frames, matches: JSON.stringify(frames) === JSON.stringify(expected) };
  });
  return { executable: path.resolve(executable), icon: path.resolve(icon), executableSha256: sha(exe), expectedFrames: expected, groups, matches: groups.length > 0 && groups.every((g) => g.matches) };
}

function verifyIcon(executable, icon) {
  const result = inspectIcon(executable, icon);
  if (!result.matches) {
    const error = new Error('Embedded Windows icon differs from icons/icon.ico. Rebuild the app resources before distributing this executable.');
    error.result = result;
    throw error;
  }
  return result;
}

if (require.main === module) {
  const root = path.resolve(__dirname, '..');
  const name = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).name;
  const executable = process.argv[2] || path.join(root, 'src-tauri', 'target', 'release', `${name}.exe`);
  const icon = process.argv[3] || path.join(root, 'src-tauri', 'icons', 'icon.ico');
  try {
    const result = verifyIcon(executable, icon);
    console.log(`Verified embedded icon: ${name} (${result.expectedFrames.map((f) => f.width).join(', ')} px)`);
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
module.exports = { icoFrames, peResources, inspectIcon, verifyIcon };
