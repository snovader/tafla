# Releasing

Every package shares one version, which `VERSION` holds, and one changelog,
`CHANGELOG.md`. Pushing a tag `v<version>` runs
[the release workflow](.github/workflows/release.yml), which checks each
language and publishes its packages:

| Registry        | Packages                         | Authentication                                            |
| --------------- | -------------------------------- | --------------------------------------------------------- |
| npm             | `tafla`                          | Trusted publishing, with provenance                       |
| PyPI            | `tafla`                          | Trusted publishing                                        |
| crates.io       | `tafla`, then `tafla-gen`        | `CARGO_REGISTRY_TOKEN`, or trusted publishing without one |
| Maven Central   | `io.github.snovader.tafla:tafla` | Central Portal token, and a signing key                   |
| Go module proxy | `github.com/snovader/tafla/go`   | None: the workflow's token pushes the tag `go/v<version>` |

Each registry has a job of its own, so a registry that fails doesn't stop the
others, and its failed jobs can be run again. A version that a registry has
published can't be published again. The Go module's tag can't be taken back
either, so its job waits for every language's build, and a release whose
build fails in any language publishes no Go version.

A Go module is published by a tag rather than uploaded: the workflow tags the
release's commit `go/v<version>`, for the module in `go/`, and asks the Go
module proxy for it, which builds the module from the tag and keeps it.
`go/tests` and `go/benchmarks` are modules of their own, which aren't
published and get no tag, so the published module requires nothing and its
tests run from the module cache.
The module records no version, since its tag is its version, so
`scripts/set-version.sh` leaves it out. From 2.0.0 on, Go requires the major
version at the end of the module path, as in `github.com/snovader/tafla/go/v2`,
in `go/go.mod` and every import of it, and the workflow's Go check fails until
they have it.

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
version in `VERSION`. It pushes the Go module's tag, `go/v0.1.0`, itself.

## Dry runs

A pull request that touches the release files, such as `VERSION`, a package's
manifest or the workflow, runs the workflow without publishing. It checks every
language, builds the npm package, the Python distributions, the crates, the
Maven repository, signed with a throwaway key, and the Go module, which the go
command fetches from a tag `go/v<version>` in the run's checkout, and uploads
them to the workflow run. To run it by hand, run the Release workflow from the
Actions tab with `dry_run` on. A run that isn't on a tag never publishes.

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
- **Go module proxy**: no account or secret is needed. The job pushes
  `go/v<version>` with the workflow's token, so a ruleset that restricts
  creating tags must let GitHub Actions create them. Before the first
  release:
  - Make the `go-proxy` environment require an approval, from a required
    reviewer, since its job pushes a tag that publishes the module for
    good, and no secret stands in its way as the other registries' do.
  - Add `refs/tags/go/v*` to the ruleset of release tags, which protects
    `refs/tags/v*` from updates and deletion: the proxy and the checksum
    database keep the module that a tag named first, and a moved tag would
    no longer match it.
