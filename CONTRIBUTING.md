# Contributing to YouTube Lounge Client

Thank you for considering contributing to the YouTube Lounge Client! This document outlines the process for contributing to the project.

## Code of Conduct

Please be respectful and considerate of others when contributing to this project. We welcome contributors from all backgrounds and skill levels.

## How to Contribute

### Reporting Bugs

If you find a bug, please report it by creating an issue using the bug report template. Include as much information as possible, such as:

- A clear description of the bug
- Steps to reproduce the issue
- Expected vs. actual behavior
- Environment details (Rust version, OS, etc.)
- Error messages or logs

### Suggesting Features

Feature requests are welcome! To suggest a feature:

1. Check existing issues to make sure the feature hasn't already been requested
2. Create a new issue using the feature request template
3. Describe the feature and its use case clearly

### Pull Requests

We welcome pull requests for bug fixes, features, and documentation improvements:

1. Fork the repository
2. Create a new branch for your changes
3. Make your changes
4. Add tests for your changes
5. Update documentation if needed
6. Run the test suite to ensure all tests pass
7. Submit a pull request

## Development Workflow

### Setting Up the Development Environment

```bash
# Clone the repository
git clone https://github.com/bertybuttface/youtube-lounge-rs.git
cd youtube-lounge-rs

# Set up git hooks (recommended)
git config core.hooksPath .githooks

# Build the library
cargo build

# Run tests
cargo test

# Try the basic example (requires a YouTube device with pairing code)
cargo run --example basic_usage YOUR_PAIRING_CODE
```

The project includes git hooks that automatically run formatting and linting checks before each commit to ensure code quality. Using these hooks is highly recommended.

### Getting Familiar with the Codebase

We strongly encourage new contributors to run the basic example to understand how the library works. This will help you:

1. Experience the pairing process with a real YouTube device
2. See the client in action with actual commands and events
3. Understand the core functionality and API design

Running the example requires a YouTube-compatible device (Smart TV, Chromecast, etc.) with a pairing code displayed on screen. The example code is well-commented and shows the typical usage flow.

Alternatively you can use a web browser pointed at https://www.youtube.com/tv#/ but you must change the user agent to “User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:87.0) Gecko/20100101 Cobalt/87.0” it will then work just like a YouTube-compatible device.

### Code Style and Linting

#### Rust Style

- Follow the Rust style guide
- Document public API functions and types
- Use meaningful variable and function names
- Keep functions small and focused
- Write clear commit messages (follow conventional commits format)

#### Rust Formatting

- Run `cargo fmt` to automatically format Rust code
- For specific files: `cargo fmt -- path/to/file.rs`
- To check without modifying: `cargo fmt -- --check`

#### Rust Linting (Clippy)

- Run `cargo clippy -- -A clippy::await_holding_lock` to lint library code
- Run `cargo clippy --tests -- -A clippy::await_holding_lock` to lint test code
- Auto-fix issues: `cargo clippy --fix -- -A clippy::await_holding_lock`
- Auto-fix with uncommitted changes: `cargo clippy --fix --allow-dirty --allow-staged -- -A clippy::await_holding_lock`

#### Markdown Style

- Markdown files must follow the rules in `.markdownlint.json`
- The pre-commit hook will prevent commits if Markdown files don't pass linting
- To lint Markdown locally, install markdownlint-cli: `npm install -g markdownlint-cli` or `bun install -g markdownlint-cli`
- Run the linter with: `markdownlint --config .markdownlint.json "*.md" ".github/**/*.md"`
- Auto-fix issues: `markdownlint --config .markdownlint.json --fix "*.md" ".github/**/*.md"`
- Our Markdown style is based on GitHub-friendly best practices, with minimal restrictions

All these checks are automatically run by the pre-commit hook, which will provide detailed instructions if any issues are found.

### Testing

- Write unit tests for new functionality
- Ensure integration tests pass
- For features involving YouTube devices, explain how you tested with real devices or https://www.youtube.com/tv#/

## Release Process

1. Update version in Cargo.toml following semantic versioning
2. Create a git tag matching the new version
3. Push the tag to trigger the automated release process
4. GitHub Actions will:
   - Run tests and linting
   - Publish to crates.io
   - Create a GitHub release

## License

By contributing to this project, you agree that your contributions will be licensed under the project's CC BY-NC 4.0 license.