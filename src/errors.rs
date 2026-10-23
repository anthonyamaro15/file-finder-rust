use std::io;
use std::path::Path;
use thiserror::Error;

/// Main application error type that encompasses all possible errors
#[derive(Error, Debug)]
pub enum AppError {
    #[error("File system error: {0}")]
    FileSystem(#[from] io::Error),

    #[error("Configuration error: {message}")]
    Configuration { message: String },

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type alias for convenience
pub type AppResult<T> = Result<T, AppError>;

/// Specific error type for file operations with detailed context
#[derive(Error, Debug, Clone)]
pub enum FileOperationError {
    #[error("Permission denied: Cannot access '{path}'. {context}")]
    PermissionDenied { path: String, context: String },

    #[error("File not found: '{path}' does not exist")]
    FileNotFound { path: String },

    #[error("Invalid name: '{name}' is not valid. {reason}")]
    InvalidName { name: String, reason: String },

    #[error("Already exists: '{path}' already exists")]
    AlreadyExists { path: String },

    #[error("Invalid path: '{path}' is not a valid file system path")]
    InvalidPath { path: String },

    #[error("Operation not supported: {operation} is not supported on '{path}'")]
    UnsupportedOperation { operation: String, path: String },

    #[error("Disk full: Not enough space to complete operation on '{path}'")]
    DiskFull { path: String },

    #[error("IO error: {message}")]
    Io { message: String },
}

/// Result type alias for file operations
pub type FileOperationResult<T> = Result<T, FileOperationError>;

impl From<io::Error> for FileOperationError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::PermissionDenied => Self::PermissionDenied {
                path: "unknown".to_string(),
                context: "Check file permissions and ownership".to_string(),
            },
            io::ErrorKind::NotFound => Self::FileNotFound {
                path: "unknown".to_string(),
            },
            io::ErrorKind::AlreadyExists => Self::AlreadyExists {
                path: "unknown".to_string(),
            },
            io::ErrorKind::StorageFull => Self::DiskFull {
                path: "unknown".to_string(),
            },
            _ => Self::Io {
                message: error.to_string(),
            },
        }
    }
}

impl From<FileOperationError> for AppError {
    fn from(error: FileOperationError) -> Self {
        AppError::FileSystem(io::Error::other(error.to_string()))
    }
}

impl FileOperationError {
    /// Create a permission denied error with context
    pub fn permission_denied<P: AsRef<Path>>(path: P, context: &str) -> Self {
        Self::PermissionDenied {
            path: path.as_ref().display().to_string(),
            context: context.to_string(),
        }
    }

    /// Create a file not found error
    pub fn file_not_found<P: AsRef<Path>>(path: P) -> Self {
        Self::FileNotFound {
            path: path.as_ref().display().to_string(),
        }
    }

    /// Create an already exists error
    pub fn already_exists<P: AsRef<Path>>(path: P) -> Self {
        Self::AlreadyExists {
            path: path.as_ref().display().to_string(),
        }
    }

    /// Create an invalid name error
    pub fn invalid_name<S: Into<String>>(name: S, reason: &str) -> Self {
        Self::InvalidName {
            name: name.into(),
            reason: reason.to_string(),
        }
    }

    /// Check if this is a "file not found" error
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::FileNotFound { .. })
    }

    /// Get a user-friendly error message for UI display
    pub fn user_message(&self) -> String {
        match self {
            Self::PermissionDenied { path, context } => {
                let file_name = Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path);
                format!(
                    "Cannot access '{}': Permission denied. {}",
                    file_name, context
                )
            }
            Self::FileNotFound { path } => {
                let file_name = Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path);
                format!(
                    "'{}' not found. The file or directory may have been moved or deleted.",
                    file_name
                )
            }
            Self::InvalidName { name, reason } => {
                format!("'{}' is not a valid name: {}", name, reason)
            }
            Self::AlreadyExists { path } => {
                let file_name = Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path);
                format!("'{}' already exists. Choose a different name.", file_name)
            }
            Self::InvalidPath { path } => {
                format!("'{}' is not a valid file system path.", path)
            }
            Self::UnsupportedOperation { operation, path } => {
                let file_name = Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path);
                format!("{} is not supported on '{}'.", operation, file_name)
            }
            Self::DiskFull { .. } => {
                "Not enough disk space to complete the operation. Free up space and try again."
                    .to_string()
            }
            Self::Io { message } => {
                format!("File operation failed: {}", message)
            }
        }
    }
}

/// Validation functions for file operations
pub mod validation {
    use super::*;
    use std::path::Path;

    /// Validate a filename for creation/rename operations
    pub fn validate_filename(name: &str) -> FileOperationResult<()> {
        if name.is_empty() {
            return Err(FileOperationError::invalid_name(
                name,
                "Name cannot be empty",
            ));
        }

        if name.len() > 255 {
            return Err(FileOperationError::invalid_name(
                name,
                "Name too long (maximum 255 characters)",
            ));
        }

        // Check for invalid characters (Windows and Unix)
        let invalid_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];
        if name.chars().any(|c| invalid_chars.contains(&c)) {
            return Err(FileOperationError::invalid_name(
                name,
                "Contains invalid characters: / \\ : * ? \" < > |",
            ));
        }

        // Check for reserved names (Windows)
        let reserved_names = [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];
        let name_upper = name.to_uppercase();
        if reserved_names.iter().any(|&reserved| {
            name_upper == reserved || name_upper.starts_with(&format!("{}.", reserved))
        }) {
            return Err(FileOperationError::invalid_name(
                name,
                "Reserved system name",
            ));
        }

        // Check for names that are just dots
        if name == "." || name == ".." {
            return Err(FileOperationError::invalid_name(
                name,
                "Cannot use '.' or '..' as names",
            ));
        }

        // Check for trailing dots or spaces (Windows issue)
        if name.ends_with('.') || name.ends_with(' ') {
            return Err(FileOperationError::invalid_name(
                name,
                "Cannot end with dots or spaces",
            ));
        }

        Ok(())
    }

    /// Validate that a path exists and is accessible
    pub fn validate_path_exists<P: AsRef<Path>>(path: P) -> FileOperationResult<()> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(FileOperationError::file_not_found(path));
        }
        Ok(())
    }

    /// Validate that a path doesn't exist (for creation operations)
    pub fn validate_path_not_exists<P: AsRef<Path>>(path: P) -> FileOperationResult<()> {
        let path = path.as_ref();
        if path.exists() {
            return Err(FileOperationError::already_exists(path));
        }
        Ok(())
    }

    /// Validate that we have permission to perform an operation on a path
    pub fn validate_permissions<P: AsRef<Path>>(
        path: P,
        operation: &str,
    ) -> FileOperationResult<()> {
        let path = path.as_ref();

        // Check if parent directory exists and is writable for create operations
        if operation == "create" || operation == "rename" {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    return Err(FileOperationError::file_not_found(parent));
                }

                // Try to check if directory is writable by attempting to create a temp file
                // Use process ID to avoid conflicts with user files
                let temp_path = parent.join(format!(".ff_perm_test_{}", std::process::id()));
                let create_result = std::fs::File::create(&temp_path);
                if create_result.is_err() {
                    return Err(FileOperationError::permission_denied(
                        parent,
                        "Cannot write to parent directory",
                    ));
                } else {
                    // Clean up the temp file
                    let _ = std::fs::remove_file(temp_path);
                }
            }
        }

        // For delete operations, check if the file itself is writable
        if operation == "delete" && path.exists() {
            let metadata = std::fs::metadata(path).map_err(|_| {
                FileOperationError::permission_denied(path, "Cannot read file metadata")
            })?;

            if metadata.permissions().readonly() {
                return Err(FileOperationError::permission_denied(
                    path,
                    "File is read-only",
                ));
            }
        }

        Ok(())
    }
}
