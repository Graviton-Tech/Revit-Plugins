# Skills

Public Galaxy skills, published the same way as the plugin DLLs: one folder per skill, holding
`skill.json`, `SKILL.md` and `script.cs` exactly as Galaxy saved them.

- In Galaxy's **Skills** tab, open a skill's card menu and choose **Publish to GitHub folder…**;
  it copies the folder here (`Skills\<skill folder>\`). Commit and push, as for a DLL.
- The GitHub Action (`build-manifest.yml`) then writes `skills.json` at the repo root: the index
  every Galaxy reads for **Shared skills…** (scope *GitHub*). No sign-in is needed to install.
- Galaxy verifies each downloaded file against the hashes in the skill's `skill.json`, so the
  files must stay byte-for-byte as saved (`.gitattributes` here turns line-ending conversion off).
- To publish a new version: save the skill with a higher version in Galaxy, publish again (the
  folder is replaced). To take a skill down: delete its folder and push.
