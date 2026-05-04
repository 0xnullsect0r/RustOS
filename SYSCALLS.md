# RustOS Syscall Reference

This document provides comprehensive documentation for all system calls available in RustOS.

## Syscall Invocation

Syscalls are invoked using the `int 0x80` interrupt from userspace or in-kernel processes.

**Register Convention (x86_64):**
- `rax`: Syscall number
- `rdi`: First argument (arg1)
- `rsi`: Second argument (arg2)
- `rdx`: Third argument (arg3)
- **Return value**: In `rax` (negative values indicate errors)

**Example (in assembly):**
```asm
mov rax, 1         ; SYS_WRITE
mov rdi, 1         ; stdout fd
mov rsi, msg       ; message pointer
mov rdx, len       ; message length
int 0x80           ; invoke syscall
```

---

## File I/O Syscalls (0-99)

These syscalls handle basic file operations and process lifecycle.

### SYS_READ (0)

Read data from a file descriptor.

**Signature:**
```c
ssize_t read(int fd, void *buf, size_t count);
```

**Arguments:**
- `rdi` (arg1): File descriptor (0 = stdin, 1 = stdout, 2 = stderr, ≥3 = VFS files)
- `rsi` (arg2): Pointer to buffer (must be writable)
- `rdx` (arg3): Number of bytes to read

**Return Value:**
- `≥0`: Number of bytes read (may be less than requested)
- `-22`: EINVAL (invalid arguments)
- `-9`: EBADF (bad file descriptor)

**Behavior:**
- Reads from keyboard input when fd=0 (stdin)
- Non-blocking: returns immediately if no input available
- Stops at newline character (`\n` or `\r`)

**Examples:**
```c
char buf[256];
ssize_t n = read(0, buf, 256);  // Read from stdin
```

**Known Limitations:**
- Cannot read from files opened via SYS_OPEN (stateless file access only)
- Only stdin (fd=0) implemented; fd ≥3 returns EBADF

---

### SYS_WRITE (1)

Write data to a file descriptor.

**Signature:**
```c
ssize_t write(int fd, const void *buf, size_t count);
```

**Arguments:**
- `rdi` (arg1): File descriptor (1 = stdout, 2 = stderr, ≥3 = VFS files)
- `rsi` (arg2): Pointer to buffer (must be readable)
- `rdx` (arg3): Number of bytes to write

**Return Value:**
- `≥0`: Number of bytes written (equal to count on success)
- `-22`: EINVAL (invalid UTF-8 or arguments)
- `-9`: EBADF (bad file descriptor)

**Behavior:**
- fd=1: Writes to framebuffer (visible output)
- fd=2: Writes to serial debug output
- Only UTF-8 text supported; invalid UTF-8 returns EINVAL

**Examples:**
```c
const char *msg = "Hello\n";
ssize_t n = write(1, msg, 6);  // Print to stdout
```

**Known Limitations:**
- Only stdout/stderr supported; fd ≥3 returns EBADF
- Non-UTF-8 data returns error instead of partial write

---

### SYS_OPEN (2)

Check if a file exists and obtain a file descriptor.

**Signature:**
```c
int open(const char *pathname);
```

**Arguments:**
- `rdi` (arg1): Pointer to null-terminated path string

**Return Value:**
- `3`: File exists (or is a built-in `/bin` command)
- `-2`: ENOENT (file not found)
- `-22`: EINVAL (null pointer or unterminated string)

**Behavior:**
- Returns fd=3 for any existing file or `/bin` command path
- Does not actually open the file (stateless operation)
- Virtual `/bin` commands are treated as existing files

**Examples:**
```c
int fd = open("/etc/hosts");     // Returns 3 if exists, -2 if not
int fd = open("/bin/echo");      // Returns 3 (built-in command exists)
```

**Known Limitations:**
- Does not allocate a file descriptor; always returns 3 or error
- Cannot open files for read/write; only existence check
- No support for file modes or flags (read-only check)

---

### SYS_CLOSE (3)

Close a file descriptor (no-op).

**Signature:**
```c
int close(int fd);
```

**Arguments:**
- `rdi` (arg1): File descriptor to close

**Return Value:**
- `0`: Always succeeds (no-op)

**Behavior:**
- Currently a no-op; always returns success
- Exists for POSIX compatibility

**Examples:**
```c
close(3);  // Returns 0 (no effect)
```

---

### SYS_EXEC (59)

Execute a program from a file path.

**Signature:**
```c
int exec(const char *pathname);
```

**Arguments:**
- `rdi` (arg1): Pointer to null-terminated path string

**Return Value:**
- On success: Does not return (process replaced)
- `-2`: ENOENT (file not found)
- `-8`: ENOEXEC (file is not a valid ELF binary)
- `-22`: EINVAL (null pointer or unterminated string)

**Behavior:**
- Loads ELF binary from specified path
- Replaces current process image
- Searches both `/bin` virtual commands and filesystem
- Returns to shell with exit code on failure

**Examples:**
```c
exec("/bin/hello");        // Execute built-in command
exec("/usr/app/myapp");    // Execute file from VFS
```

**Known Limitations:**
- No support for command-line arguments
- No environment variables passed
- Cannot execute shell scripts (ELF only)
- stdin/stdout limited to simple console I/O

---

### SYS_EXIT (60)

Terminate the current process.

**Signature:**
```c
void exit(int status);
```

**Arguments:**
- `rdi` (arg1): Exit status code (signed 64-bit)

**Return Value:**
- Never returns (exits to kernel or shell)

**Behavior:**
- Prints exit status: `[process exited with code X]`
- Terminates the process and returns control to shell
- Negative status codes are printed as-is

**Examples:**
```c
exit(0);       // Successful exit
exit(1);       // Error exit
exit(-1);      // Negative exit code
```

---

## Process & Memory Syscalls (100-199)

Reserved for future process and memory management syscalls. Currently not implemented.

**Planned syscalls may include:**
- `fork()` / `clone()` (process creation)
- `mmap()` / `munmap()` (memory mapping)
- `brk()` / `sbrk()` (heap management)
- `wait()` / `waitpid()` (process synchronization)

---

## Network Syscalls (300-310)

These syscalls provide access to the TCP/IP stack and network operations.

### SYS_SOCKET (300)

Create a socket for network communication.

**Signature:**
```c
int socket(int domain, int type, int protocol);
```

**Arguments:**
- `rdi` (arg1): Address family (AF_INET=2, AF_INET6=10)
- `rsi` (arg2): Socket type (SOCK_STREAM=1, SOCK_DGRAM=2)
- `rdx` (arg3): Protocol (IPPROTO_TCP=6, IPPROTO_UDP=17)

**Return Value:**
- `≥0`: Socket file descriptor
- `-22`: EINVAL (invalid domain/type/protocol)
- `-24`: EMFILE (too many open sockets)

**Behavior:**
- Creates a socket for TCP or UDP communication
- Returns a file descriptor for use with other socket syscalls

**Examples:**
```c
int sock = socket(2, 1, 6);     // AF_INET, SOCK_STREAM (TCP)
int sock = socket(2, 2, 17);    // AF_INET, SOCK_DGRAM (UDP)
```

---

### SYS_BIND (301)

Bind a socket to a local address and port.

**Signature:**
```c
int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen);
```

**Arguments:**
- `rdi` (arg1): Socket file descriptor
- `rsi` (arg2): Pointer to sockaddr structure
- `rdx` (arg3): Length of sockaddr structure (typically 16 bytes)

**Return Value:**
- `0`: Success
- `-22`: EINVAL (invalid arguments)
- `-98`: EADDRINUSE (address already in use)
- `-9`: EBADF (bad file descriptor)

**sockaddr Structure (IPv4):**
```c
struct sockaddr_in {
    short sin_family;           // AF_INET (2)
    unsigned short sin_port;    // Port in network byte order (htons)
    struct in_addr sin_addr;    // IP address
    char sin_zero[8];           // Padding
};
```

**Examples:**
```c
struct sockaddr_in addr;
addr.sin_family = 2;                // AF_INET
addr.sin_port = htons(8080);        // Port 8080
addr.sin_addr.s_addr = htonl(0);    // Bind to any interface

int result = bind(sockfd, (struct sockaddr *)&addr, 16);
```

---

### SYS_LISTEN (302)

Listen for incoming connections on a socket.

**Signature:**
```c
int listen(int sockfd, int backlog);
```

**Arguments:**
- `rdi` (arg1): Socket file descriptor
- `rsi` (arg2): Maximum pending connections (backlog)

**Return Value:**
- `0`: Success
- `-22`: EINVAL (sockfd is not a socket)
- `-9`: EBADF (bad file descriptor)

**Behavior:**
- Marks socket as listening for incoming connections
- Queues incoming connections up to backlog count

**Examples:**
```c
listen(sockfd, 5);  // Listen with backlog of 5
```

---

### SYS_CONNECT (303)

Connect a socket to a remote address.

**Signature:**
```c
int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen);
```

**Arguments:**
- `rdi` (arg1): Socket file descriptor
- `rsi` (arg2): Pointer to remote sockaddr
- `rdx` (arg3): Length of sockaddr structure

**Return Value:**
- `0`: Success
- `-111`: ECONNREFUSED (connection refused)
- `-113`: EHOSTUNREACH (host unreachable)
- `-22`: EINVAL (invalid arguments)
- `-9`: EBADF (bad file descriptor)

**Examples:**
```c
struct sockaddr_in server;
server.sin_family = 2;                      // AF_INET
server.sin_port = htons(80);                // Port 80
server.sin_addr.s_addr = inet_aton("8.8.8.8");

int result = connect(sockfd, (struct sockaddr *)&server, 16);
```

---

### SYS_ACCEPT (304)

Accept an incoming connection on a listening socket.

**Signature:**
```c
int accept(int sockfd, struct sockaddr *addr, socklen_t *addrlen);
```

**Arguments:**
- `rdi` (arg1): Listening socket file descriptor
- `rsi` (arg2): Pointer to sockaddr for client (optional, can be NULL)
- `rdx` (arg3): Pointer to addrlen variable

**Return Value:**
- `≥0`: New socket file descriptor for accepted connection
- `-11`: EAGAIN (no connections available; would block)
- `-22`: EINVAL (sockfd is not listening)
- `-9`: EBADF (bad file descriptor)

**Behavior:**
- Accepts a pending connection from the backlog
- Returns a new socket file descriptor for communication
- Blocks until a connection arrives

**Examples:**
```c
struct sockaddr_in client;
socklen_t len = sizeof(client);
int client_sock = accept(listen_sock, (struct sockaddr *)&client, &len);
```

---

### SYS_SEND (305)

Send data on a connected socket.

**Signature:**
```c
ssize_t send(int sockfd, const void *buf, size_t len, int flags);
```

**Arguments:**
- `rdi` (arg1): Socket file descriptor
- `rsi` (arg2): Pointer to data buffer
- `rdx` (arg3): Number of bytes to send
- `rcx` (arg4): Flags (typically 0; MSG_DONTWAIT=0x40)

**Return Value:**
- `≥0`: Number of bytes sent
- `-11`: EAGAIN (would block; socket not ready)
- `-22`: EINVAL (invalid arguments)
- `-9`: EBADF (bad file descriptor)
- `-107`: ENOTCONN (socket not connected)

**Examples:**
```c
const char *msg = "GET / HTTP/1.0\r\n\r\n";
ssize_t sent = send(sock, msg, strlen(msg), 0);
```

---

### SYS_RECV (306)

Receive data on a connected socket.

**Signature:**
```c
ssize_t recv(int sockfd, void *buf, size_t len, int flags);
```

**Arguments:**
- `rdi` (arg1): Socket file descriptor
- `rsi` (arg2): Pointer to receive buffer
- `rdx` (arg3): Maximum bytes to read
- `rcx` (arg4): Flags (typically 0; MSG_DONTWAIT=0x40)

**Return Value:**
- `>0`: Number of bytes received
- `0`: Connection closed by peer
- `-11`: EAGAIN (would block; no data available)
- `-22`: EINVAL (invalid arguments)
- `-9`: EBADF (bad file descriptor)
- `-107`: ENOTCONN (socket not connected)

**Examples:**
```c
char buf[4096];
ssize_t n = recv(sock, buf, sizeof(buf), 0);
if (n > 0) {
    // Process received data
}
```

---

### SYS_SETSOCKOPT (307)

Set socket options.

**Signature:**
```c
int setsockopt(int sockfd, int level, int optname, const void *optval, socklen_t optlen);
```

**Arguments:**
- `rdi` (arg1): Socket file descriptor
- `rsi` (arg2): Option level (SOL_SOCKET=1, IPPROTO_TCP=6)
- `rdx` (arg3): Option name (SO_REUSEADDR=2, TCP_NODELAY=1)
- `rcx` (arg4): Pointer to option value
- Stack (5th): Length of option value

**Return Value:**
- `0`: Success
- `-22`: EINVAL (invalid option)
- `-9`: EBADF (bad file descriptor)

**Common Options:**
- `SO_REUSEADDR`: Allow reuse of TIME_WAIT addresses
- `SO_RCVTIMEO`: Receive timeout
- `SO_SNDTIMEO`: Send timeout
- `TCP_NODELAY`: Disable Nagle algorithm

**Examples:**
```c
int reuse = 1;
setsockopt(sock, 1, 2, &reuse, sizeof(reuse));  // SO_REUSEADDR
```

---

### SYS_GETSOCKOPT (308)

Get socket options.

**Signature:**
```c
int getsockopt(int sockfd, int level, int optname, void *optval, socklen_t *optlen);
```

**Arguments:**
- `rdi` (arg1): Socket file descriptor
- `rsi` (arg2): Option level (SOL_SOCKET=1, IPPROTO_TCP=6)
- `rdx` (arg3): Option name
- `rcx` (arg4): Pointer to value buffer
- Stack (5th): Pointer to optlen (in/out)

**Return Value:**
- `0`: Success
- `-22`: EINVAL (invalid option)
- `-9`: EBADF (bad file descriptor)

**Examples:**
```c
int reuse;
socklen_t len = sizeof(reuse);
getsockopt(sock, 1, 2, &reuse, &len);  // SO_REUSEADDR
```

---

### SYS_SHUTDOWN (309)

Shut down part of a socket connection.

**Signature:**
```c
int shutdown(int sockfd, int how);
```

**Arguments:**
- `rdi` (arg1): Socket file descriptor
- `rsi` (arg2): How (SHUT_RD=0, SHUT_WR=1, SHUT_RDWR=2)

**Return Value:**
- `0`: Success
- `-22`: EINVAL (invalid how)
- `-9`: EBADF (bad file descriptor)
- `-107`: ENOTCONN (socket not connected)

**Behavior:**
- `SHUT_RD=0`: Disable further receives
- `SHUT_WR=1`: Disable further sends
- `SHUT_RDWR=2`: Disable both

**Examples:**
```c
shutdown(sock, 2);  // Shutdown both directions
```

---

### SYS_CLOSE_SOCKET (310)

Close a socket file descriptor.

**Signature:**
```c
int close(int sockfd);
```

**Arguments:**
- `rdi` (arg1): Socket file descriptor

**Return Value:**
- `0`: Success
- `-9`: EBADF (bad file descriptor)

**Behavior:**
- Closes the socket and releases resources
- Any further operations on the socket fail

**Examples:**
```c
close(sock);
```

---

## Error Codes

Negative return values indicate errors. The error code is the negative of the standard errno value:

| Error | Code | Meaning |
|-------|------|---------|
| EPERM | -1 | Operation not permitted |
| ENOENT | -2 | No such file or directory |
| ESRCH | -3 | No such process |
| EINTR | -4 | Interrupted system call |
| EIO | -5 | I/O error |
| ENXIO | -6 | No such device or address |
| E2BIG | -7 | Argument list too long |
| ENOEXEC | -8 | Exec format error |
| EBADF | -9 | Bad file descriptor |
| ECHILD | -10 | No child processes |
| EAGAIN | -11 | Resource temporarily unavailable |
| ENOMEM | -12 | Cannot allocate memory |
| EACCES | -13 | Permission denied |
| EFAULT | -14 | Bad address |
| EBUSY | -16 | Device or resource busy |
| EEXIST | -17 | File exists |
| ENODEV | -19 | No such device |
| ENOTDIR | -20 | Not a directory |
| EISDIR | -21 | Is a directory |
| EINVAL | -22 | Invalid argument |
| ENFILE | -23 | Too many open files in system |
| EMFILE | -24 | Too many open files |
| ENOTTY | -25 | Not a typewriter |
| EADDRINUSE | -98 | Address already in use |
| ECONNREFUSED | -111 | Connection refused |
| EHOSTUNREACH | -113 | No route to host |
| ENOTCONN | -107 | Socket is not connected |
| ENOSYS | -38 | Function not implemented |

---

## Syscall Table

| Number | Name | Purpose | Status |
|--------|------|---------|--------|
| 0 | read | Read from file descriptor | ✅ Implemented |
| 1 | write | Write to file descriptor | ✅ Implemented |
| 2 | open | Check file existence | ✅ Implemented |
| 3 | close | Close file descriptor | ✅ Implemented (no-op) |
| 59 | execve | Execute program | ✅ Implemented |
| 60 | exit | Exit process | ✅ Implemented |
| 100-299 | (reserved) | Process/memory management | ⏸️ Reserved |
| 300 | socket | Create socket | ✅ Implemented |
| 301 | bind | Bind socket | ✅ Implemented |
| 302 | listen | Listen for connections | ✅ Implemented |
| 303 | connect | Connect to remote | ✅ Implemented |
| 304 | accept | Accept connection | ✅ Implemented |
| 305 | send | Send data | ✅ Implemented |
| 306 | recv | Receive data | ✅ Implemented |
| 307 | setsockopt | Set socket option | ✅ Implemented |
| 308 | getsockopt | Get socket option | ✅ Implemented |
| 309 | shutdown | Shutdown socket | ✅ Implemented |
| 310 | close_socket | Close socket | ✅ Implemented |

---

## Writing Userspace Programs

Use the `rustos-rt` crate to write userspace programs that call these syscalls:

```rust
use rustos_rt::syscall::*;

fn main() {
    let msg = b"Hello from userspace!\n";
    write(FD_STDOUT, msg);
    exit(0);
}
```

The `rustos-rt` crate provides:
- `read()`, `write()`, `open()`, `close()` — File I/O
- `exec()`, `exit()` — Process control
- `socket()`, `bind()`, `listen()`, etc. — Network operations
- Proper errno handling and Rust-friendly wrappers

---

## Related Documentation

- [SHELL_COMMANDS.md](SHELL_COMMANDS.md) - Shell command reference
- [LIMITATIONS.md](LIMITATIONS.md) - Known limitations
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues
