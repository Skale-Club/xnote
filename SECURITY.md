# Security

Xnote handles meeting audio, transcripts and the API keys you give it, so we take security reports
seriously.

## Reporting a problem

Please **do not open a public issue** for a security problem. Use GitHub's
[private vulnerability reporting](https://github.com/Skale-Club/xnote/security/advisories/new)
instead, with:

- the Xnote version (shown at the bottom of the sidebar) and your Windows version;
- what an attacker could do, and the steps to reproduce it.

Never include real API keys, transcripts or personal data in a report.

## What is in scope

- API keys leaving the Windows Credential Manager, or being written to logs or files.
- Audio or text sent somewhere the settings do not say.
- Code execution through meeting files, model output, updates or downloads.
- Bypassing the update signature or the download checksums.

Only the latest release receives fixes.
