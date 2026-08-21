#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
import AdmZip from 'adm-zip';
import { readFile, writeFile } from 'fs/promises';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');

const tag = process.argv[2] || process.env.PLUGIN_VERSION || 'v0.0.0';
const version = tag.startsWith('v') ? tag.slice(1) : tag;

const manifestPath = path.join(root, 'manifest.json');
const bundlePath = path.join(root, 'dist', 'index.js');
const outPath = path.join(root, `livekit-meet-plugin-${tag}.zip`);

// Read manifest, set version to the release version, and include it in the zip.
const manifest = JSON.parse(await readFile(manifestPath, 'utf-8'));
manifest.version = version;

const zip = new AdmZip();
zip.addFile('manifest.json', Buffer.from(JSON.stringify(manifest, null, 2), 'utf-8'));
zip.addLocalFile(bundlePath, '', 'index.js');
await writeFile(outPath, zip.toBuffer());

console.log(`Created ${outPath}`);
