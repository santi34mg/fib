# Control Flow

## if / else

Conditionally execute a block based on a boolean expression:

```
if condition {
    then_body
}

if condition {
    then_body
} else {
    else_body
}
```

**Example**:

```
if x > 0 {
    printf("positive\n")
} else {
    printf("non-positive\n")
}
```

## for

C-style `for` loop. All three clauses are optional:

```
for (initializer; condition; post_operation) {
    body
}
```

Omitting all clauses creates an infinite loop:

```
for (;;) {
    // infinite loop
}
```

**Example**:

```
for (var sint32 i = 0; i < 10; i += 1) {
    printf("%d\n", i)
}
```

## break

`break` exits the nearest enclosing `for` loop immediately:

```
for (;;) {
    if done { break }
}
```

## continue

`continue` skips the rest of the current loop iteration and proceeds to the next:

```
for (var sint32 i = 0; i < 10; i += 1) {
    if i == 5 { continue }
    printf("%d\n", i)
}
```

## defer

`defer` schedules a statement to execute when the enclosing function returns. Multiple deferred statements execute in reverse order (last-in, first-out):

```
defer statement
```

**Example**:

```
fn main() sint32 {
    defer printf("this prints last\n")
    defer printf("this prints first\n")
    printf("this prints immediately\n")
    return 0
}
```
