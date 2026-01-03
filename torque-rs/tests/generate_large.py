#!/usr/bin/env python3
"""Generate a large Torque test file to stress-test the parser."""

import sys

def generate_macro(i):
    return f'''
macro TestMacro{i}(a: Number, b: Number): Number {{
    let x: Number = a + b * {i};
    let y: Number = x - {i * 2};
    if (x > y) {{
        return x;
    }} else {{
        return y;
    }}
}}
'''

def generate_builtin(i):
    return f'''
javascript builtin TestBuiltin{i}(context: Context, receiver: JSAny, arg1: Number, arg2: String): JSAny {{
    let result: Number = 0;
    let temp: Number = arg1 + {i};

    typeswitch (receiver) {{
        case (n: Number): {{
            result = n + temp;
        }}
        case (s: String): {{
            result = {i};
        }}
        case (o: JSObject): {{
            result = o.length;
        }}
    }}

    while (result < 100) {{
        result = result + 1;
        if (result == 50) {{
            continue;
        }}
        if (result > 75) {{
            break;
        }}
    }}

    return result > 0 ? result : 0;
}}
'''

def generate_namespace(i, content):
    return f'''
namespace test{i} {{
{content}
}}
'''

def generate_type(i):
    return f'type TestType{i} extends Object generates "TNode<TestType{i}>";\n'

def generate_const(i):
    return f'const kTestConst{i}: Number = {i * 100};\n'

def main():
    lines = []
    lines.append("// Auto-generated stress test file\n")
    lines.append("// This file tests the parser with a large amount of Torque code\n\n")

    # Generate types
    for i in range(200):
        lines.append(generate_type(i))

    lines.append("\n")

    # Generate consts
    for i in range(200):
        lines.append(generate_const(i))

    # Generate macros
    for i in range(600):
        lines.append(generate_macro(i))

    # Generate builtins
    for i in range(300):
        lines.append(generate_builtin(i))

    # Generate namespaces with content
    for i in range(50):
        content = ""
        for j in range(10):
            content += generate_macro(i * 100 + j)
        lines.append(generate_namespace(i, content))

    content = "".join(lines)
    print(f"Generated {len(content)} characters, {content.count(chr(10))} lines", file=sys.stderr)
    print(content)

if __name__ == "__main__":
    main()
