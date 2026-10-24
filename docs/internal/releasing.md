# Release Hayai

Hayai releases are built by GitHub Actions from version tags. Do not upload
locally built binaries to a GitHub release: the workflow is the reproducible
source of release artifacts and checksums.

## Prepare

1. Choose the next semantic version, such as `0.1.1` for fixes after `0.1.0`.
2. Update `package.version` in `Cargo.toml` and run `cargo check --locked` so
   `Cargo.lock` records the same Hayai version.
3. If changing Scry, publish its release first, update the `scry-client` Git tag
   deliberately, refresh `Cargo.lock`, and verify file search against that
   released daemon.
4. Keep `CHANGELOG.md`'s Unreleased section current as changes merge. Before
   releasing, move those entries under `## <version> - YYYY-MM-DD` and leave an
   empty Unreleased section for the next cycle. Check the entries against the
   commits since the previous tag. Update README status or user documentation
   for changed behavior.
5. Run the contribution checks and the manual launcher checklist in
   `CONTRIBUTING.md`.
6. Merge through a green pull request.

## Publish

Tag the exact reviewed merge commit and push the tag:

```powershell
git tag -a v0.1.1 -m "Hayai 0.1.1"
git push origin v0.1.1
```

The tag must equal `v` followed by the `Cargo.toml` version. The release
workflow rejects a mismatch, a missing or duplicate dated changelog entry, or
an empty entry for that version. Preview the exact release notes locally with:

```powershell
./scripts/export-release-notes.ps1 -Version 0.1.1 -OutputPath release-notes.md
```

The workflow then:

1. restores Rust build caches;
2. installs the pinned Velopack CLI;
3. builds Hayai with its lockfile;
4. downloads the independently built Scry package matching the locked tag;
5. creates the Velopack installer and portable ZIP;
6. publishes the versioned setup, portable archive, and checksum manifest;
7. publishes the matching `CHANGELOG.md` entry as the GitHub release notes.

The public artifacts use the tag in their names:

```text
hayai-v<version>-windows-x86_64-setup.exe
hayai-v<version>-windows-x86_64-portable.zip
hayai-v<version>-windows-x86_64-SHA256SUMS.txt
```

The release title is `Hayai <version>`, matching Scry Search's release-title
convention without exposing Velopack's internal package naming.

Versions containing `-` are automatically marked as prereleases.

## Verify the published artifacts

On a Windows machine without a development checkout:

1. Compare both downloads with `SHA256SUMS.txt`.
2. Install Hayai and confirm its shortcuts and launch-at-login entry.
3. Run the manual window checks in `CONTRIBUTING.md`.
4. Enter file mode, install Scry through the offered action, and confirm a file
   query returns results.
5. Open the contextual action panel on an app and a file.
6. Install the release over the previous release and verify settings/startup
   behavior survives.
7. Uninstall Hayai and verify its launch-at-login entry is removed.
8. Extract the portable ZIP separately and confirm it launches without an
   installer-created shortcut.

If packaging fails after a tag is pushed, fix the cause through a pull request
and publish a new version. Do not move or reuse a published tag.

## Local packaging

`scripts/build-installer.ps1` uses the Scry release tag recorded in `Cargo.lock`
by default. It caches the downloaded package under `target/scry-release` so
subsequent builds can reuse it.

An explicit downloaded Scry release archive can be supplied for offline builds:

```powershell
./scripts/build-installer.ps1 -ScryPackage ../scry-search-v0.1.0-alpha.1-windows-x86_64.zip
```

The archive must contain Scry's daemon, CLI, and install/uninstall scripts. The
normal online path is preferred because its tag is derived directly from
Hayai's lockfile.
