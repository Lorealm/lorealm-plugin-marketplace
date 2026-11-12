# Lorealm plugin marketplace

The plugins a [Lorealm](https://github.com/Lorealm/Lorealm) server offers to install, and the tool that checks them.

Each plugin has one file in [`plugins/`](plugins), named after its handle. It says where the plugin is developed and lists the releases that were reviewed:

```toml
repository = "https://github.com/jane/word-count"

[[versions]]
version = "0.2.0"
url = "https://github.com/jane/word-count/releases/download/v0.2.0/word_count-0.2.0.zip"
sha256 = "6035cdcea157eb7e9b7a0ac94db2ee51fabcdf4aaa6edf4998b4a6653d94f62b"
```

Everything else Lorealm shows about a plugin (its name, description, permissions, README) comes from the release itself.

## Listing a plugin, or a new version

Every version is listed with its own pull request.

1. Release the plugin. With [`lorealm-plugin`](https://github.com/Lorealm/lorealm-plugin-cli), that's `lorealm-plugin stage 0.2.0` and `lorealm-plugin publish`: the plugin's release workflow builds it and attaches a zip to a GitHub release.
2. Fork this repository, and from it run:

   ```sh
   cargo run -- add https://github.com/you/your-plugin/releases/download/v0.2.0/your_plugin-0.2.0.zip
   ```

   It downloads the zip, checks it, and writes the version into `plugins/<handle>.toml`.
3. Open a pull request with that file.

A plugin's handle is its file's name, so it goes to whoever lists it first.

## Rules for the Marketplace
This marketplace is the official index displayed to all users, and must therefore adhere
to stricter guidelines:

1. The plugin code must be hosted on https://github.com/
2. The plugin repository must contain a pre-built release
3. The plugin is not allowed to send telemetry data, user details or other data that may
otherwise be considered a breach of security/privacy, to a 3rd party address.
4. The plugin may not include code meant to purposefully slow down or degrade the user's
experience when using Lorealm.
5. The plugin repository must include a LICENSE file with anOSI-approved
(https://opensource.org/licenses) open-source license.

Failing to meet any of these requirements will result in your Pull Request being closed.

You may open a new pull request at any time if you've fixed any violations to the rules
above.

## What's checked

The `Check plugins` workflow runs on every pull request that touches `plugins/`, and fails unless:

- the zip is attached to a release of the plugin's own repository, and its sha256 is the one listed;
- its `manifest.toml` is valid, with the file's handle and the listed version;
- it holds the built backend, as a WebAssembly component, and every frontend entry its manifest names, so a server installs it without building anything;
- every version already listed is left exactly as it was.

The run's summary then shows, for each version added, what it asks for: its permissions, the hosts it may send requests to, where it appears in the client, and what it gains over the version before it.

A second workflow, `Flag changes to what's listed`, comments on any pull request that does more than add: one that removes a plugin, or removes or alters a version that's already listed, such as swapping an earlier version's hash while adding a new one. Its check fails too, and the comment is updated once the pull request only adds again.

That is what a review looks at. A listed zip isn't rebuilt from its source, so a review doesn't prove the two match; what protects a user is the sandbox plugins run in and the permissions they have to ask for. The hash guarantees that what's installed is the zip that was reviewed.

## The index

On every push to `main`, the `Publish the index` workflow builds `index.json` and publishes it with GitHub Pages, next to each plugin's README (`readme/<handle>.md`, from its newest version, with links into the repository made absolute).

```json
{
  "schema": 1,
  "plugins": [
    {
      "handle": "word_count",
      "repository": "https://github.com/jane/word-count",
      "readme": "readme/word_count.md",
      "versions": [
        {
          "version": "0.2.0",
          "api": "0.1.0",
          "url": "https://github.com/jane/word-count/releases/download/v0.2.0/word_count-0.2.0.zip",
          "sha256": "6035cd…",
          "name": "Word Count",
          "description": "Counts words in replies.",
          "author": "Jane",
          "tags": [],
          "permissions": ["chat:read"],
          "hosts": [],
          "slots": [{ "slot": "message", "title": "Word Count", "unsafe": false }],
          "requires": {},
          "recommends": [],
          "conflicts": []
        }
      ]
    }
  ]
}
```

Versions are newest first. `api` is the plugin API a version was built for, and a server only runs versions built for its own, so a client offers the newest version whose `api` matches. A release that can no longer be downloaded is left out of the index, without affecting the rest.

## The tool

```sh
cargo run -- add <zip link>                 # list a release
cargo run -- check plugins/<handle>.toml    # check a plugin file, as the workflow does
cargo run -- additive --previous <old plugins> --current <new plugins>   # what a change takes back
cargo run -- build                          # write site/index.json and site/readme/
cargo test
```

## Licensing
This repository is licensed under MIT. That covers the tool and the plugin files submitted to plugins/. It does not cover the plugins themselves, which are released by their authors under their own licenses.
