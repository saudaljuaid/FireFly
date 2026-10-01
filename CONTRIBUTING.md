# Contributing to Inkbird

Thanks for helping improve Inkbird and Phos.

1. Check existing issues before starting a substantial change. Open an issue to discuss a new feature or a change in direction.
2. Keep pull requests focused. Explain what changed, why it changed, and how you tested it.
3. Add tests when changing parser, loader, style, layout, or paint behavior. Keep the pinned upstream fixtures and their expected outputs unchanged.
4. Run the checks before opening a pull request:

   ```sh
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test --locked
   ```

For parser behavior and fixture results, see [engine status](docs/ENGINE_STATUS.md). Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).
