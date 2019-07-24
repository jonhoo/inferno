# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.0] - 2019-07-24
### Added
- Changelog
- Support for collapsing the output of the `sample` tool on macOS (#133 by [@jasonrhansen](https://github.com/jasonrhansen)).
- Multi-core stack collapsing for _major_ speedups (#128 by [@bcmyers](https://github.com/bcmyers)).
- Support for "fluid drawing" of the SVG (#136 by [@jasonrhansen](https://github.com/jasonrhansen)).
- Make zoom and search part of browser history (#121 from [@AnderEnder](https://github.com/AnderEnder)).
  This is a backport of https://github.com/brendangregg/FlameGraph/pull/198 by [@versable](https://github.com/versable).
- The `--demangle` flag to collapsers for "re-doing" broken symbol demangling from DTrace or perf (#132 by [@jasonrhansen](https://github.com/jasonrhansen)).
- Unit tests for semantic coloring.
  JavaScript: #129 by [@jordins](https://github.com/jordins)
  Java: #131 by [@jkurian](https://github.com/jkurian)
- Cirrus CI for FreeBSD CI (#124 from [@AnderEnder](https://github.com/AnderEnder))

### Changed
- Moved to `IndexMap` and FNV hashing (#127)
- Moved CI to Azure DevOps Pipelines

[Unreleased]: https://github.com/jonhoo/inferno/compare/v0.8.0...HEAD
[0.8.0]: https://github.com/jonhoo/inferno/compare/v0.7.0...v0.8.0
