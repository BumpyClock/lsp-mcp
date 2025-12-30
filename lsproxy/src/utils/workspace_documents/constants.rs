// ABOUTME: Language-specific file patterns and extensions for workspace scanning
// ABOUTME: Constants used for language detection and file discovery across supported languages

pub const DEFAULT_EXCLUDE_PATTERNS: &[&str] = &[
    "**/node_modules",
    "**/__pycache__",
    "**/.*",
    "**/dist",
    "**/target",
    "**/build",
    ".git",
];

pub const PYTHON_ROOT_FILES: &[&str] = &[
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "requirements.txt",
    "Pipfile",
    "pyrightconfig.json",
];

pub const PYTHON_FILE_PATTERNS: &[&str] = &["**/*.py", "**/*.pyx", "**/*.pyi"];
pub const PYTHON_EXTENSIONS: &[&str] = &["py", "pyx", "pyi"];

pub const TYPESCRIPT_AND_JAVASCRIPT_ROOT_FILES: &[&str] =
    &["tsconfig.json", "jsconfig.json", "package.json"];

pub const TYPESCRIPT_AND_JAVASCRIPT_FILE_PATTERNS: &[&str] =
    &["**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"];
pub const TYPESCRIPT_EXTENSIONS: &[&str] = &["ts"];
pub const TYPESCRIPTREACT_EXTENSIONS: &[&str] = &["tsx"];
pub const JAVASCRIPT_EXTENSIONS: &[&str] = &["js"];
pub const JAVASCRIPTREACT_EXTENSIONS: &[&str] = &["jsx"];
pub const TYPESCRIPT_AND_JAVASCRIPT_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx"];

pub const RUBY_ROOT_FILES: &[&str] = &[
    "Gemfile",
    "Rakefile",
    ".ruby-version",
    "config.ru",
    ".gemspec",
];
pub const RUBY_FILE_PATTERNS: &[&str] = &["**/*.rb", "**/*.erb"];
pub const RUBY_EXTENSIONS: &[&str] = &["rb", "erb"];

pub const RUST_ROOT_FILES: &[&str] = &["Cargo.toml"];
pub const RUST_FILE_PATTERNS: &[&str] = &["**/*.rs"];
pub const RUST_EXTENSIONS: &[&str] = &["rs"];

pub const CPP_ROOT_FILES: &[&str] = &[
    "makefile",
    ".clangd",
    ".clang-tidy",
    ".clang-format",
    "compile_commands.json",
    "compile_flags.txt",
    "configure.ac",
    ".git",
];
pub const C_AND_CPP_FILE_PATTERNS: &[&str] = &[
    "**/*.cpp", "**/*.cc", "**/*.c", "**/*.cxx", "**/*.h", "**/*.hpp", "**/*.hxx", "**/*.hh",
];

pub const C_EXTENSIONS: &[&str] = &["c", "h"];
pub const CPP_EXTENSIONS: &[&str] = &["cpp", "cc", "cxx", "h", "hpp", "hxx", "hh"];
pub const C_AND_CPP_EXTENSIONS: &[&str] = &["cpp", "cc", "c", "cxx", "h", "hpp", "hxx", "hh"];

pub const CSHARP_ROOT_FILES: &[&str] = &["*.sln", "*.csproj", "*.vcxproj"];
pub const CSHARP_FILE_PATTERNS: &[&str] = &["**/*.cs"];
pub const CSHARP_EXTENSIONS: &[&str] = &["cs"];

pub const JAVA_ROOT_FILES: &[&str] = &["gradlew", ".git", "mvnw"];
pub const JAVA_FILE_PATTERNS: &[&str] = &["**/*.java"];
pub const JAVA_EXTENSIONS: &[&str] = &["java"];

pub const GOLANG_ROOT_FILES: &[&str] = &["go.mod", "go.work"];
pub const GOLANG_FILE_PATTERNS: &[&str] = &["**/*.go", "**/*.gomod", "**/*.gowork", "**/*.gotmpl"];
pub const GOLANG_EXTENSIONS: &[&str] = &["go"];

pub const PHP_ROOT_FILES: &[&str] = &[
    "composer.json",
    "composer.lock",
    "phpunit.xml",
    "artisan",
    ".env",
    "index.php",
    "wp-config.php",
];
pub const PHP_FILE_PATTERNS: &[&str] = &[
    "**/*.php",
    "**/*.phtml",
    "**/*.phps",
    "**/*.php5",
    "**/*.php7",
    "**/*.php8",
];
pub const PHP_EXTENSIONS: &[&str] = &["php", "phtml", "phps", "php5", "php7", "php8"];
