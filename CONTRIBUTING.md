# Contributing to Canto

Thank you for wanting to contribute to Canto! Whether you are a developer, designer, translator, tester, or just someone who wants to help out, there is a place for you here. Every contribution, big or small, helps make Canto a better experience.

> [!IMPORTANT]
> The [GNOME Code of Conduct](https://conduct.gnome.org) applies to this project. Please be aware of it before interacting with this repository.

> [!NOTE]
> Canto is developed with AI assistance. AI-assisted changes are welcome when you built and tested them locally; say which parts were AI-written.

## Ways to Contribute

### 1. Submitting Issues or Managing the Issue Queue

Helping with issues is a great way to get involved. If you hit a bug, have an idea for a feature, or spot something that could be improved, opening an issue is a good first step. A clear bug report with steps to reproduce the problem is a huge help to the developers working on fixes.

Beyond filing new issues, you can also help manage the existing queue. Reproducing bugs that others have reported, adding useful context, identifying duplicates, or answering questions from other users all help keep things moving.

### 2. Creating Art or Design Mockups

Design contributions are welcome, including mockups for new features, refinements to icons and graphics, and improvements to the application's visual identity. For substantial design changes, please open an issue beforehand to discuss and agree on the approach.

### 3. Developing Features or Fixing Bugs

Contributors working on code, people and AI agents alike, should begin with [AGENTS.md](AGENTS.md): the commands, the hard rules and what the tests cannot see. The [Development Guide](doc/Development.md) then covers environment setup, the build process, feature flags, translations and the pull request workflow.

### 4. Translating

Translations are `.po` files in `po/`, sent as pull requests like any other change. There is no Weblate project. The [Translations](doc/Development.md#translations) section of the Development Guide has the commands. Review every entry marked fuzzy: gettext guesses those from similar strings, and they are often wrong.

### 5. Testing Releases

Testing releases is a valuable way to contribute at any level. Trying pre-release builds, verifying that reported bugs have been fixed, and running Canto across different hardware and Linux distributions all help surface platform-specific issues. Please report any findings in an issue.

## How to Create an Issue

Before creating an issue, please check the tracker for an existing report of the same problem. If one exists, add your details there instead of opening a duplicate. Otherwise, open a new issue. A good issue includes:

- A clear, descriptive title summarizing the problem or suggestion
- What you observed and what you expected to happen instead
- For bugs, the steps to reproduce it, plus your operating system, desktop environment, and Canto version
- Screenshots or logs, if they help illustrate the problem

Clear, detailed issues are far easier to act on, so include as much relevant context as you can.

## Starting a Discussion for Big Ideas

If you have a larger idea that goes beyond a simple bug fix or small feature request, open a discussion instead of an issue. Discussions are the right place for proposals that involve significant changes to the architecture, user experience, or project direction. They give the community space to explore the idea, ask questions, and refine the approach before any code is written.

A good discussion post covers:

- The motivation behind the idea
- The problem it solves
- A proposed approach, at a high level

You do not need all the answers up front. The goal is to start a conversation and gather input from others who might be affected by or interested in the change.

## Finding a Good First Issue

Not sure where to start? Look for issues labeled ["Good first issue"](https://github.com/gusthanks/canto/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22good%20first%20issue%22) in the tracker. These are approachable tasks for newcomers: well-scoped, limited in complexity, or with clear requirements.

Do not hesitate to reach out if you need guidance. Leave a comment on the issue to ask questions, or reach out to a maintainer or contributor directly. Everyone was new to the project at some point, and we are happy to help you get started.
