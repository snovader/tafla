# Releasing

Every package shares one version, which `VERSION` holds, and one changelog,
`CHANGELOG.md`. Pushing a tag `v<version>` runs
[the release workflow](.github/workflows/release.yml), which checks each
language and publishes its packages:

| Registry      | Packages                         | Authentication                                            |
| ------------- | -------------------------------- | --------------------------------------------------------- |
| npm           | `tafla`                          | Trusted publishing, with provenance                       |
| PyPI          | `tafla`                          | Trusted publishing                                        |
| crates.io     | `tafla`, then `tafla-gen`        | `CARGO_REGISTRY_TOKEN`, or trusted publishing without one |
| Maven Central | `io.github.snovader.tafla:tafla` | Central Portal token, and a signing key                   |

Each registry has jobs of its own, so a registry that fails doesn't stop the
others, and its failed jobs can be run again. A version that a registry has
published can't be published again.

## The Python distributions

PyPI's `tafla` has a native implementation, written in Rust, and a pure-Python
one. The release publishes:

- Native wheels for Linux x86_64 and aarch64, as `manylinux_2_28` and
  `musllinux_1_2`, macOS x86_64 and arm64, and Windows x64. Each platform has
  an `abi3` wheel for CPython 3.14 and later, a `cp314t` wheel for
  free-threaded CPython 3.14, and an `abi3t` wheel for free-threaded CPython
  3.15 and later.
- A pure-Python `py3-none-any` wheel, which pip installs on any other platform
  or Python.
- The source distribution, which builds the native module with Rust. pip
  builds from it only when told to, since the pure-Python wheel installs
  wherever no native one does.

`python/scripts/test-wheels.sh` tests each wheel on its own, in each Python
that it's for, and the job that publishes them runs once every one has passed.

## Releasing a version

1. Set the version, which writes it into every package's manifest and the
   lock files:

   ```sh
   scripts/set-version.sh 0.1.0
   ```

2. In `CHANGELOG.md`, rename the Unreleased section to the version and its
   date, such as `## [0.1.0] - 2026-11-07`, start a new Unreleased section
   above it, and update the links at the end.

3. Open a pull request with both. The release workflow checks that every
   package has the version, and builds every artifact without publishing it.

4. Once it's merged, tag the merge commit on `main` and push the tag:

   ```sh
   git switch main
   git pull
   git tag -a v0.1.0 -m "tafla 0.1.0"
   git push origin v0.1.0
   ```

The workflow fails before publishing anything if the tag isn't `v` and the
version in `VERSION`.

## Dry runs

A pull request that touches the release files, such as `VERSION`, a package's
manifest or the workflow, runs the workflow without publishing. It checks every
language, builds the npm package, the Python distributions for every platform,
each tested, the crates and the Maven repository, signed with a throwaway key,
and uploads them to the workflow run. To run it by hand, run the Release workflow from the Actions tab
with `dry_run` on. A run that isn't on a tag never publishes.

## Setting up the registries

The workflow's publishing jobs run in the GitHub environments `npm`, `pypi`,
`crates-io` and `maven-central`, which can hold their secrets and require an
approval before a job runs.

- **npm** trusts a publisher only for a package that exists. Publish the first
  release with an `NPM_TOKEN` secret, a granular access token that may publish
  `tafla`. Then add a trusted publisher on the package's settings page, with
  the repository `snovader/tafla`, the workflow `release.yml` and the
  environment `npm`, and delete the secret.
- **PyPI**: add a pending trusted publisher for the project `tafla`, with the
  repository `snovader/tafla`, the workflow `release.yml` and the environment
  `pypi`. No secret is needed.
- **crates.io** also trusts a publisher only for a crate that exists. Publish
  the first release with a `CARGO_REGISTRY_TOKEN` secret, a token with the
  `publish-new` and `publish-update` scopes. Then add a trusted publisher to
  `tafla` and `tafla-gen`, with the repository `snovader/tafla`, the workflow
  `release.yml` and the environment `crates-io`, and delete the secret.
- **Maven Central**: verify the namespace `io.github.snovader` on the
  [Central Portal](https://central.sonatype.com/), and generate a user token,
  whose username and password are the `MAVEN_CENTRAL_USERNAME` and
  `MAVEN_CENTRAL_PASSWORD` secrets. Central rejects unsigned artifacts, so
  publishing fails without a signing key: `SIGNING_KEY` is an ASCII-armored
  secret key, from `gpg --armor --export-secret-keys <key id>`, whose public
  key is on a key server such as `keys.openpgp.org`, and `SIGNING_PASSWORD` is
  its passphrase. The workflow publishes the deployment once the portal has
  validated it.
