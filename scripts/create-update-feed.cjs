// Release manifest consumed by the portable app, hosted beside its signed exe.
const fs = require('node:fs');
const path = require('node:path');
function create({ version, url, signature, notes = '' }) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('Use a stable semantic version.');
  const source = new URL(url);
  if (source.protocol !== 'https:' || source.username || source.password) throw new Error('The download must use HTTPS.');
  const decoded = Buffer.from(signature.trim(), 'base64').toString('utf8');
  if (!decoded.startsWith('untrusted comment:') || !decoded.includes('trusted comment:')) throw new Error('A Tauri/minisign signature is required.');
  return { version, notes, url, signature: signature.trim() };
}
module.exports = { create };
if (require.main === module) {
  try {
    const config = JSON.parse(fs.readFileSync(path.join(__dirname, '../src-tauri/tauri.conf.json'), 'utf8'));
    const dir = process.argv[2] || 'dist';
    const app = config.productName;
    const slug = JSON.parse(fs.readFileSync(path.join(__dirname, '../package.json'), 'utf8')).name;
    const signature = fs.readFileSync(path.join(dir, `${app}.exe.sig`), 'utf8');
    const manifest = create({ version: config.version, url: `https://github.com/Skale-Club/${slug}/releases/download/v${config.version}/${app}.exe`, signature, notes: 'Internal downloads and signed portable updates. Settings and app data are preserved.' });
    fs.writeFileSync(path.join(dir, 'latest.json'), JSON.stringify(manifest, null, 2) + '\n');
    console.log(`Created signed update feed for ${app} ${manifest.version}`);
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
