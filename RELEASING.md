# Releasing

Each of tafla's packages has a version and a changelog of its own, and is
released on its own. A package's version describes its API and behavior.
Whether two implementations read and write the same tables is a matter of the
format, which [SPEC.md](SPEC.md) describes and the conformance suite checks,
not of their versions. A change to the format changes every implementation,
as [AGENTS.md](AGENTS.md) describes, and each releases it in a version of its
own.

| Package      | Published as                                           | Version                        | Changelog                      |
| ------------ | ------------------------------------------------------ | ------------------------------ | ------------------------------ |
| `typescript` | `tafla` on npm                                         | `typescript/core/package.json` | `typescript/core/CHANGELOG.md` |
| `python`     | `tafla` on PyPI                                        | `python/core/pyproject.toml`   | `python/core/CHANGELOG.md`     |
| `java`       | `io.github.snovader.tafla:tafla` on Maven Central      | `java/core/build.gradle.kts`   | `java/core/CHANGELOG.md`       |
| `rust`       | `tafla` on crates.io                                   | `rust/core/Cargo.toml`         | `rust/core/CHANGELOG.md`       |
| `tafla-gen`  | `tafla-gen` on crates.io                               | `rust/gen/Cargo.toml`          | `rust/gen/CHANGELOG.md`        |
| `go`         | `github.com/snovader/tafla/go` on the Go module proxy  | Its tag, `go/v<version>`       | `go/CHANGELOG.md`              |

The C++ headers aren't published yet, and have no version.

[The release workflow](.github/workflows/release.yml) releases one package at
a time. A run on `main`, for a package and its version, checks the package,
builds its artifacts, and publishes them, and then tags the commit
`<package>/v<version>`, such as `python/v0.2.0`. Each registry has jobs of its
own, and a job that fails can be run again. A version that a registry has
published can't be published again.

## Versions that depend on each other

- tafla-gen generates code that calls a language's runtime. When the code
  that it generates needs a newer runtime, that runtime is released first, and
  tafla-gen's changelog names the release that its code needs.
- tafla-gen's tests run the Rust that it generates against the Rust runtime,
  at the version that its manifest names, which `scripts/version.sh rust`
  writes. crates.io resolves that version when tafla-gen is published, so
  tafla-gen's release follows the release of that version of `tafla`.
- The Python runtime's native classes build on the Rust runtime in the
  repository, rather than on a release of it, so each release of `tafla` on
  PyPI holds the Rust runtime of its commit.

## The Go module

A Go module is published by a tag rather than uploaded: the tag
`go/v<version>`, for the module in `go/`, publishes it, and the Go module
proxy builds the module from the tag and keeps it. The tag can't be taken
back, so the workflow pushes it, once the module has built from it in the
run's checkout, rather than a person pushing it first. `go/tests` and
`go/benchmarks` are modules of their own, which aren't published and get no
tag, so the published module requires nothing and its tests run from the
module cache.

The module records no version, since its tag is its version, so its release
names the version, and `scripts/version.sh` has none to set. From 2.0.0 on,
Go requires the major version at the end of the module path, as in
`github.com/snovader/tafla/go/v2`, in `go/go.mod` and every import of it, and
the workflow's Go check fails until they have it.

## The Python distributions

PyPI's `tafla` has a native implementation, written in Rust, and a pure-Python
one. The release publishes:

- Native wheels for Linux x86_64 and aarch64, as `manylinux_2_28` and
  `musllinux_1_2`, macOS x86_64 and arm64, and Windows x64. Each platform has
  an `abi3` wheel for CPython 3.14 and later, a `cp314t` wheel for
  free-threaded CPython 3.14, and an `abi3t` wheel for CPython 3.15 and
  later, which loads with the GIL too, and which pip prefers there to the
  `abi3` one.
- A pure-Python `py3-none-any` wheel, which pip installs on any other platform
  or Python.
- The source distribution, which builds the native module with Rust. pip
  builds from it only when told to, since the pure-Python wheel installs
  wherever no native one does. Without Rust, uv_build builds the pure-Python
  wheel from the same sources, as `python/README.md` shows.

`python/scripts/test-wheels.sh` tests each wheel on its own, in each Python
that it's for, and the job that publishes them runs once every one has passed.

## Releasing a version

1. Set the package's version, which writes it into its manifest and the lock
   files that record it:

   ```sh
   scripts/version.sh python 0.2.0
   ```

   `scripts/version.sh python` prints the version, and fails if those files
   disagree on it. The Go module has no version to set.

2. In the package's changelog, rename the Unreleased section to the version
   and its date, such as `## [0.2.0] - 2026-11-20`, and start a new Unreleased
   section above it. Update the links at the end, which compare the package's
   tags, such as `python/v0.1.0...python/v0.2.0`, and the last tag with
   `main`.

3. Open a pull request with both. The release workflow builds the package, as
   a release of the version would, without publishing it.

4. Once it's merged, run the Release workflow on `main`, from the Actions tab
   or with `gh`, for the package and its version, with `dry_run` off:

   ```sh
   gh workflow run release.yml --ref main -f package=python -f version=0.2.0 -f dry_run=false
   ```

   The publish job waits for an approval in the registry's environment, and
   once the package is published, the workflow tags the commit
   `python/v0.2.0`.

The workflow fails before it builds if the version isn't the one in the
package's manifest, or if the package already has a tag for the version on
another commit. A job that fails can be run again, and the jobs that push a
tag find the tag that they pushed before.

## Dry runs

A dry run checks a package and builds its artifacts without publishing them:
the npm package, the Python distributions for every platform, each tested,
the crates, the Maven repository, signed with a throwaway key, or the Go
module, which the go command fetches from a tag `go/v<version>` in the run's
checkout. It uploads them to the workflow run.

A pull request runs one for each package whose version it changes. One that
changes the workflow, `.github/actions/release-tag` or `scripts/version.sh`
runs one for every package, and one that changes
`python/scripts/test-wheels.sh` or `go/go.mod` runs one for Python or Go. The
Go module has no version until its release names one, so a pull request's dry
run of it takes the first version of its major version, such as `0.0.0`.
Every pull request checks that each package's manifest and lock files agree
on its version.

Other changes to how a package builds, such as to its manifest, are checked
by its language's workflow, and the dry run of its next version builds them
for every platform. To run one sooner, run the Release workflow by hand with
`dry_run` on, from any branch: a run that isn't on `main` never publishes.

## Setting up the registries

The workflow's publishing jobs run in the GitHub environments `npm`, `pypi`,
`crates-io`, `maven-central` and `go-proxy`, which can hold their secrets and
require an approval before a job runs.

- **npm** trusts a publisher only for a package that exists. Publish the first
  release with an `NPM_TOKEN` secret, a granular access token that may publish
  `tafla`. Then add a trusted publisher on the package's settings page, with
  the repository `snovader/tafla`, the workflow `release.yml` and the
  environment `npm`, and delete the secret.
- **PyPI**: add a pending trusted publisher for the project `tafla`, with the
  repository `snovader/tafla`, the workflow `release.yml` and the environment
  `pypi`. No secret is needed.
- **crates.io** also trusts a publisher only for a crate that exists. Publish
  the first release of each crate with a `CARGO_REGISTRY_TOKEN` secret, a
  token with the `publish-new` and `publish-update` scopes. Then add a trusted
  publisher to `tafla` and `tafla-gen`, with the repository `snovader/tafla`,
  the workflow `release.yml` and the environment `crates-io`, and delete the
  secret.
- **Maven Central**: verify the namespace `io.github.snovader` on the
  [Central Portal](https://central.sonatype.com/), and generate a user token,
  whose username and password are the `MAVEN_CENTRAL_USERNAME` and
  `MAVEN_CENTRAL_PASSWORD` secrets. Central rejects unsigned artifacts, so
  publishing fails without a signing key: `SIGNING_KEY` is an ASCII-armored
  secret key, from `gpg --armor --export-secret-keys <key id>`, whose public
  key is on a key server such as `keys.openpgp.org`, and `SIGNING_PASSWORD` is
  its passphrase. The workflow publishes the deployment once the portal has
  validated it.
- **Go module proxy**: no account or secret is needed. Make the `go-proxy`
  environment require an approval, from a required reviewer, since its job
  pushes the tag that publishes the module for good, and no secret stands in
  its way as the other registries' do.

The workflow pushes every release's tag, `<package>/v<version>`, with its
token. Before the first release:

- Add `refs/tags/*/v*` to a ruleset that protects tags from updates and
  deletion, since a tag records the commit that a version was published from,
  and the Go module proxy and the checksum database keep the module that a
  tag named first. If the ruleset also restricts creating tags, let GitHub
  Actions create them.
- Limit each environment's deployment branches to `main`, as the workflow
  publishes only from `main`.
