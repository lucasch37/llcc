# llcc

llcc is a (toy) C compiler written in Rust with no dependencies that compiles C code into x86_64 following Intel syntax.

## Features
### Current/Planned:
- [x] Binary and unary operators
- [x] Global and local variable declarations
- [x] `char`, `int`, `float`, & `double` types
- [ ] Function calls & recursion
- [ ] Pointers
- [ ] `putchar`
- [ ] Conditional statements
- [ ] `for`, `while` loops

### Error Reporting
Features detailed, colorful error messages and can report multiple errors in a program.

<table>
  <tr>
    <th>Code</th>
    <th>Messages</th>
  </tr>
  <tr>
  <td>
    
  ```C
void foo() {
    return 0;
}

int main() {
    int x = 7 / 0;
    return 0;
}
  ```
  </td>
  <td>
    <img width="486" height="233" alt="image" src="https://github.com/user-attachments/assets/2f3e22ff-0e57-432b-951b-8cdfa1d94e98" />
  </td>
</tr>
</table>

### AST Pretty Printing
Use the `--print-ast` option to print the AST tree after parsing.

<table>
  <tr>
    <th>Code</th>
    <th>Tree</th>
  </tr>
  <tr>
  <td>
    
  ```C
int main() {
    int y = 9 * 2;
    return 0;
}
  ```
  </td>
  <td>
    <img width="428" height="765" alt="image" src="https://github.com/user-attachments/assets/93ee6c84-5b47-460b-a72e-abffa573cd83" />
  </td>
</tr>
</table>
