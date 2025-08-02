# Contributing to P2P-Play

Thank you for your interest in contributing to P2P-Play! 🎉

P2P-Play is a peer-to-peer story sharing application built with Rust and libp2p. This project serves as both a functional P2P application and a playground for experimenting with peer-to-peer networking concepts and AI-assisted development workflows. We welcome contributions of all kinds!

## 🚀 Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable version)
- Git

### Setting Up Your Development Environment

1. Fork and clone the repository:
   ```bash
   git clone https://github.com/your-username/p2p-play.git
   cd p2p-play
   ```

2. Build the project:
   ```bash
   cargo build
   ```

3. Run the tests:
   ```bash
   # Run all tests using the test runner script
   ./scripts/test_runner.sh

   # Or run tests directly with cargo
   cargo test
   ```

4. Try running the application:
   ```bash
   cargo run
   ```

## 🛠️ Development Workflow

### Code Standards

Before submitting any changes, please ensure your code meets our standards:

1. **Format your code:**
   ```bash
   cargo fmt --check
   # If formatting is needed:
   cargo fmt
   ```

2. **Run linting:**
   ```bash
   cargo clippy
   ```

3. **Update the changelog:**
   - Add an entry to `CHANGELOG.md` for any changes made

4. **Test your changes:**
   ```bash
   ./scripts/test_runner.sh
   ```

### Making Changes

1. Create a new branch for your feature or fix:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. Make your changes with focused, atomic commits
3. Test your changes thoroughly
4. Update documentation if needed
5. Add tests for new functionality

## 📝 Submitting Issues

We use GitHub Issues to track bugs, feature requests, and general discussions. When creating an issue, please:

1. **Use our issue templates** - They help provide the information we need
2. **Be descriptive** - Include steps to reproduce, expected behavior, and actual behavior
3. **Search existing issues** first to avoid duplicates
4. **Use appropriate labels** to categorize your issue

### Types of Issues We Welcome

- 🐛 **Bug Reports** - Found a problem? Let us know!
- ✨ **Feature Requests** - Have an idea for improvement?
- 📖 **Documentation** - Help improve our docs
- 🤔 **Questions** - Need help understanding something?
- 💡 **Experiments** - Ideas for P2P networking experiments

## 🔄 Submitting Pull Requests

1. **Create an issue first** (unless it's a trivial change) to discuss the proposed changes
2. **Fork the repository** and create your feature branch
3. **Make your changes** following our code standards
4. **Add tests** for new functionality
5. **Update documentation** as needed
6. **Fill out our PR template** completely
7. **Reference the related issue** in your PR description

### Pull Request Requirements

- [ ] Code builds successfully (`cargo build`)
- [ ] All tests pass (`cargo test`)
- [ ] Code is properly formatted (`cargo fmt --check`)
- [ ] Linting passes (`cargo clippy`)
- [ ] `CHANGELOG.md` is updated
- [ ] Documentation is updated (if applicable)
- [ ] Related issue is referenced

## 🏗️ Project Architecture

P2P-Play is organized into several key components:

- **Network Layer** (`src/network.rs`) - P2P networking with libp2p
- **Storage** (`src/storage.rs`) - Local file system and database operations
- **Handlers** (`src/handlers.rs`) - Command and event processing
- **UI** (`src/ui.rs`) - Command-line interface
- **Types** (`src/types.rs`) - Core data structures

For more details, see the architecture overview in the project README.

## 🧪 Testing Philosophy

We encourage comprehensive testing:

- **Unit tests** for individual components
- **Integration tests** for end-to-end functionality
- **Manual testing** for user experience validation

## 💬 Communication

- **GitHub Issues** - For bugs, features, and general discussion
- **Pull Request Reviews** - For code discussion and collaboration
- **README and Documentation** - For project information

## 🎯 What We're Looking For

We especially welcome contributions in these areas:

- **P2P Networking Features** - New protocols, peer discovery improvements
- **Story Management** - Enhanced story creation, sharing, and organization
- **User Experience** - CLI improvements, error handling, user feedback
- **Testing** - More comprehensive test coverage
- **Documentation** - Better examples, tutorials, and guides
- **Performance** - Optimizations and profiling
- **Experiments** - Novel P2P concepts and implementations

## 📜 Code of Conduct

Please note that this project is governed by our [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold this code. Please report unacceptable behavior to the project maintainers.

## 🎉 Recognition

All contributors will be recognized for their efforts. We maintain a record of contributions and appreciate every improvement, no matter how small!

---

**Happy coding, and thank you for helping make P2P-Play better!** 🚀