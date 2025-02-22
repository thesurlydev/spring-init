# spring-init

A powerful CLI tool for creating and managing Spring Boot projects with AI-assisted dependency selection. This tool streamlines the process of initializing Spring Boot applications by automatically suggesting dependencies based on your project requirements.

![image](assets/image.png)

## Features

- **AI-Powered Dependency Selection**: Analyzes your Product Requirements Document (PRD) to suggest relevant Spring Boot dependencies
- **Project Initialization**: Creates new Spring Boot projects with customizable configurations
- **Build Management**: Handles project building and Maven plugin synchronization
- **Project Reset**: Ability to reset project state when needed
- **Dependency Information**: Lists all available Spring Boot dependencies

## Installation

Ensure you have Rust installed on your system, then clone and build the project:

```bash
git clone https://github.com/thesurlydev/spring-init.git
cd spring-init
cargo build --release
```

## Configuration

Create a `config.json` file in your project root with the following structure:

```json
{
    "boot_version": "3.2.0",
    "java_version": "21",
    "app_name": "my-spring-app",
    "app_version": "0.0.1-SNAPSHOT",
    "package_name": "com.example.demo",
    "projects_dir": "./projects",
    "maven_plugins": [],
    "include_deps": []
}
```

## Usage

### Initialize a New Project

```bash
# Basic initialization
spring-init init

# Initialize with PRD-based dependency suggestions
spring-init init --prd path/to/prd.md

# Initialize with additional dependencies
spring-init init --include web,data-jpa,postgresql
```

### Get Dependency Suggestions

```bash
spring-init suggest-deps --prd path/to/prd.md
```

### List Available Dependencies

```bash
spring-init deps
```

### Build Project

```bash
spring-init build
```

### Show Project Information

```bash
spring-init info
```

### Reset Project

```bash
spring-init reset
```

## PRD Format

When using the AI-powered dependency suggestion feature, your PRD should clearly describe your application's requirements and features. The AI will analyze this document to suggest appropriate Spring Boot dependencies.

## Dependencies

The tool integrates with [start.spring.io](https://start.spring.io) to provide access to all official Spring Boot dependencies. For a complete list of available dependencies, use the `deps` command.