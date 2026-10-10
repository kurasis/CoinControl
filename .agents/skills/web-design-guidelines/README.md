# Vendored Vercel web-design-guidelines

This repository-local skill is available from
[SKILL.md](SKILL.md). Its upstream instructions are preserved byte-for-byte as
ordinary tracked files; it uses no symlinks or home-directory installation.
This README is local provenance documentation, not part of the upstream skill.

## Source and pinned revision

- Repository: <https://github.com/vercel-labs/agent-skills>
- Requested source: <https://github.com/vercel-labs/agent-skills/tree/main/skills/web-design-guidelines>
- Upstream commit: `063bee94c3f4df8453406c830b0a7df0f2860278`
- Pinned directory: <https://github.com/vercel-labs/agent-skills/tree/063bee94c3f4df8453406c830b0a7df0f2860278/skills/web-design-guidelines>
- Copied on: 2026-10-10
- Skill metadata: name `web-design-guidelines`, author `vercel`, version `1.0.0`.

The complete upstream directory at this revision contains only `SKILL.md`;
there are no bundled helper files. Its SHA-256 is
`f4647ca866a3accf763777f83e7682954f0187cd6bea7eea0399796652414e8f`.
The file is excluded from Prettier to preserve its original formatting.

## Current guidelines and network check

The skill requires fresh rules before each review from:

<https://raw.githubusercontent.com/vercel-labs/web-interface-guidelines/main/command.md>

On 2026-10-10 this exact URL was fetched successfully: **HTTP 200**, 8055 bytes,
no URL change after redirects, SHA-256
`d246b026f4f29b5823a9cc857f9edf3d2507002e055e32040cadeaf3b38e0234`.
This records endpoint availability, not an application UI audit. The rules are
intentionally fetched live rather than replaced with a bundled stale snapshot;
their contents may change independently of the pinned skill revision.

Source retrieval uses `github.com`; live rule retrieval uses
`raw.githubusercontent.com`. If a later request is blocked, report that limitation
and the required domain rather than claiming a completed review.

## Reproduce or update

To retrieve this exact upstream revision into a fresh temporary checkout:

```sh
git clone https://github.com/vercel-labs/agent-skills.git /tmp/vercel-agent-skills-source
git -C /tmp/vercel-agent-skills-source checkout --detach 063bee94c3f4df8453406c830b0a7df0f2860278
git -C /tmp/vercel-agent-skills-source ls-tree -r HEAD -- skills/web-design-guidelines
```

For an update, choose and review a new full commit SHA, check out that revision,
and inspect its complete `skills/web-design-guidelines/` directory. Copy all its
tracked files into `.agents/skills/web-design-guidelines/` as ordinary files,
preserving their bytes and executable modes. Remove only previously copied
upstream files confirmed absent from the new revision; retain local provenance.
If upstream adds its own README, move this local provenance to a separate README
before copying it so the upstream file remains intact.

Record the new source commit, inventory and hashes here. Validate the YAML
frontmatter (`name`, `description` and metadata), compare every copied file with
the pinned Git blobs, and confirm there are no symlinks. Re-fetch the live rules
and record the actual response; retain or extend formatting exclusions only for
verbatim upstream files. Check formatting of this README and `AGENTS.md`, then
review the diff before committing. Application code is outside this installation.
