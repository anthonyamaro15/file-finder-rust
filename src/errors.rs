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

    #[error("Path error: {message}")]
    Path { message: String },

    #[error("Image processing error: {message}")]
    Image { message: String },

    #[error("Terminal/UI error: {message}")]
    Terminal { message: String },

    #[error("Clipboard error: {message}")]
    Clipboard { message: String },

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IDE validation error: '{ide}' is not supported. Valid options: nvim, vscode, zed")]
    InvalidIDE { ide: String },

    #[error("Directory traversal error: {message}")]
    DirectoryTraversal { message: String },

    #[error("Cache error: {message}")]
    Cache { message: String },
}

/// Result type alias for convenience
pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    /// Create a configuration error
    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    /// Create a path error
    pub fn path<S: Into<String>>(message: S) -> Self {
        Self::Path {
            message: message.into(),
        }
    }

    /// Create an image processing error
    pub fn image<S: Into<String>>(message: S) -> Self {
        Self::Image {
            message: message.into(),
        }
    }

    /// Create a terminal/UI error
    pub fn terminal<S: Into<String>>(message: S) -> Self {
        Self::Terminal {
            message: message.into(),
        }
    }

    /// Create a clipboard error
    pub fn clipboard<S: Into<String>>(message: S) -> Self {
        Self::Clipboard {
            message: message.into(),
        }
    }

    /// Create an IDE validation error
    pub fn invalid_ide<S: Into<String>>(ide: S) -> Self {
        Self::InvalidIDE { ide: ide.into() }
    }

    /// Create a directory traversal error
    pub fn directory_traversal<S: Into<String>>(message: S) -> Self {
        Self::DirectoryTraversal {
            message: message.into(),
        }
    }

    /// Create a cache error
    pub fn cache<S: Into<String>>(message: S) -> Self {
        Self::Cache {
            message: message.into(),
        }
    }

    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            AppError::FileSystem(_) => true,        // Can retry file operations
            AppError::Configuration { .. } => true, // Can use defaults
            AppError::Cache { .. } => true,         // Can rebuild cache
            AppError::Clipboard { .. } => true,     // Can fall back to terminal output
            AppError::Image { .. } => true,         // Can skip image preview
            AppError::DirectoryTraversal { .. } => true, // Can try different directory
            AppError::Path { .. } => false,         // Usually indicates corrupt data
            AppError::Terminal { .. } => false,     // Terminal issues are usually fatal
            AppError::Json(_) => false,             // Indicates corrupt config
            AppError::InvalidIDE { .. } => false,   // User input validation error
        }
    }

    /// Get a user-friendly message for display in UI
    pub fn user_message(&self) -> String {
        match self {
            AppError::FileSystem(e) => match e.kind() {
                io::ErrorKind::NotFound => "File or directory not found".to_string(),
                io::ErrorKind::PermissionDenied => "Permission denied".to_string(),
                io::ErrorKind::AlreadyExists => "File or directory already exists".to_string(),
                _ => format!("File system error: {}", e),
            },
            AppError::Configuration { message } => {
                format!("Configuration issue: {}", message)
            }
            AppError::Path { message } => {
                format!("Path issue: {}", message)
            }
            AppError::Image { message } => {
                format!("Cannot display image: {}", message)
            }
            AppError::Terminal { message } => {
                format!("Terminal error: {}", message)
            }
            AppError::Clipboard { message } => {
                format!("Clipboard error: {}", message)
            }
            AppError::Json(_) => "Configuration file is corrupted. Using defaults.".to_string(),
            AppError::InvalidIDE { ide } => {
                format!(
                    "'{}' is not a supported editor. Use: nvim, vscode, or zed",
                    ide
                )
            }
            AppError::DirectoryTraversal { message } => {
                format!("Cannot access directory: {}", message)
            }
            AppError::Cache { message } => {
                format!("Cache issue: {}", message)
            }
        }
    }
}

/// Helper trait to convert path-related operations to AppError
pub trait PathErrorExt<T> {
    fn path_context<S: Into<String>>(self, message: S) -> AppResult<T>;
}

impl<T> PathErrorExt<T> for Option<T> {
    fn path_context<S: Into<String>>(self, message: S) -> AppResult<T> {
        self.ok_or_else(|| AppError::path(message))
    }
}

impl<T, E> PathErrorExt<T> for Result<T, E>
where
    E: std::fmt::Display,
{
    fn path_context<S: Into<String>>(self, message: S) -> AppResult<T> {
        self.map_err(|_| AppError::path(message))
    }
}

/// Specific error type for file operations with detailed context
#[derive(Error, Debug, Clone)]
pub enum FileOperationError {
    #[error("Permission denied: Cannot access '{path}'. {context}")]
    PermissionDenied { path: String, context: String },

    #[error("File not found: '{path}' does not exist")]
    FileNotFound { path: String },

    #[error("Directory not empty: '{path}' contains files and cannot be deleted")]
    DirectoryNotEmpty { path: String },

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

    #[error("File too large: '{path}' is too large for this operation")]
    FileTooLarge { path: String },

    #[error("Cross-device operation: Cannot move '{from}' to '{to}' (different filesystems)")]
    CrossDevice { from: String, to: String },

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
        AppError::FileSystem(io::Error::new(io::ErrorKind::Other, error.to_string()))
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

    /// Create a directory not empty error
    pub fn directory_not_empty<P: AsRef<Path>>(path: P) -> Self {
        Self::DirectoryNotEmpty {
            path: path.as_ref().display().to_string(),
        }
    }

    /// Create an invalid path error
    pub fn invalid_path<P: AsRef<Path>>(path: P) -> Self {
        Self::InvalidPath {
            path: path.as_ref().display().to_string(),
        }
    }

    /// Create an unsupported operation error
    pub fn unsupported_operation<P: AsRef<Path>>(operation: &str, path: P) -> Self {
        Self::UnsupportedOperation {
            operation: operation.to_string(),
            path: path.as_ref().display().to_string(),
        }
    }

    /// Create a cross-device error
    pub fn cross_device<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Self {
        Self::CrossDevice {
            from: from.as_ref().display().to_string(),
            to: to.as_ref().display().to_string(),
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
            Self::DirectoryNotEmpty { path } => {
                let dir_name = Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path);
                format!(
                    "Cannot delete '{}': Directory is not empty. Delete contents first.",
                    dir_name
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
            Self::DiskFull { path } => {
                format!(
                    "Not enough disk space to complete the operation. Free up space and try again."
                )
            }
            Self::FileTooLarge { path } => {
                let file_name = Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path);
                format!("'{}' is too large for this operation.", file_name)
            }
            Self::CrossDevice { from, to } => {
                format!("Cannot move between different file systems. Copy and delete instead.")
            }
            Self::Io { message } => {
                format!("File operation failed: {}", message)
            }
        }
    }

    /// Get a short title for the error (for dialog titles)
    pub fn title(&self) -> &'static str {
        match self {
            Self::PermissionDenied { .. } => "Permission Denied",
            Self::FileNotFound { .. } => "File Not Found",
            Self::DirectoryNotEmpty { .. } => "Directory Not Empty",
            Self::InvalidName { .. } => "Invalid Name",
            Self::AlreadyExists { .. } => "Already Exists",
            Self::InvalidPath { .. } => "Invalid Path",
            Self::UnsupportedOperation { .. } => "Operation Not Supported",
            Self::DiskFull { .. } => "Disk Full",
            Self::FileTooLarge { .. } => "File Too Large",
            Self::CrossDevice { .. } => "Cross-Device Operation",
            Self::Io { .. } => "File Error",
        }
    }

    /// Check if this error is recoverable (user can retry)
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::PermissionDenied { .. } => false, // Need to fix permissions first
            Self::FileNotFound { .. } => false,     // File is gone
            Self::DirectoryNotEmpty { .. } => true, // Can delete contents first
            Self::InvalidName { .. } => true,       // Can choose different name
            Self::AlreadyExists { .. } => true,     // Can choose different name
            Self::InvalidPath { .. } => true,       // Can choose different path
            Self::UnsupportedOperation { .. } => false, // Operation not possible
            Self::DiskFull { .. } => true,          // Can free up space
            Self::FileTooLarge { .. } => false,     // Size won't change
            Self::CrossDevice { .. } => true,       // Can copy instead of move
            Self::Io { .. } => true,                // Might be temporary
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
                if let Err(_) = std::fs::File::create(&temp_path) {
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
