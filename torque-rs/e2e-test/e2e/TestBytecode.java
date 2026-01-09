package e2e;

import org.objectweb.asm.*;
import static org.objectweb.asm.Opcodes.*;

/**
 * End-to-end test: Generate bytecode using ASM and verify it works.
 * This simulates what our Torque-generated code does.
 */
public class TestBytecode {

    /**
     * Build an 'add' method: public static int add(int a, int b) { return a + b; }
     */
    public static void build_add(ClassWriter cw) {
        MethodVisitor mv = cw.visitMethod(
            ACC_PUBLIC | ACC_STATIC,
            "add",
            "(II)I",
            null,
            null);
        mv.visitCode();
        mv.visitVarInsn(ILOAD, 0);  // a
        mv.visitVarInsn(ILOAD, 1);  // b
        mv.visitInsn(IADD);
        mv.visitInsn(IRETURN);
        mv.visitMaxs(2, 2);
        mv.visitEnd();
    }

    /**
     * Build a 'factorial' method with loop:
     * public static int factorial(int n) {
     *     int result = 1;
     *     while (n > 1) { result = result * n; n = n - 1; }
     *     return result;
     * }
     */
    public static void build_factorial(ClassWriter cw) {
        MethodVisitor mv = cw.visitMethod(
            ACC_PUBLIC | ACC_STATIC,
            "factorial",
            "(I)I",
            null,
            null);
        mv.visitCode();

        // int result = 1
        mv.visitInsn(ICONST_1);
        mv.visitVarInsn(ISTORE, 1);  // result in local 1

        Label loopStart = new Label();
        Label loopEnd = new Label();

        mv.visitLabel(loopStart);
        // if (n <= 1) goto loopEnd
        mv.visitVarInsn(ILOAD, 0);  // n
        mv.visitInsn(ICONST_1);
        mv.visitJumpInsn(IF_ICMPLE, loopEnd);

        // result = result * n
        mv.visitVarInsn(ILOAD, 1);  // result
        mv.visitVarInsn(ILOAD, 0);  // n
        mv.visitInsn(IMUL);
        mv.visitVarInsn(ISTORE, 1);  // result

        // n = n - 1
        mv.visitVarInsn(ILOAD, 0);  // n
        mv.visitInsn(ICONST_1);
        mv.visitInsn(ISUB);
        mv.visitVarInsn(ISTORE, 0);  // n

        mv.visitJumpInsn(GOTO, loopStart);

        mv.visitLabel(loopEnd);
        mv.visitVarInsn(ILOAD, 1);  // result
        mv.visitInsn(IRETURN);

        mv.visitMaxs(2, 2);
        mv.visitEnd();
    }

    public static void main(String[] args) throws Exception {
        System.out.println("=== ASM End-to-End Test ===");

        // Create a new class dynamically
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_FRAMES | ClassWriter.COMPUTE_MAXS);
        cw.visit(V11, ACC_PUBLIC, "e2e/GeneratedMath", null, "java/lang/Object", null);

        // Add default constructor
        MethodVisitor constructor = cw.visitMethod(ACC_PUBLIC, "<init>", "()V", null, null);
        constructor.visitCode();
        constructor.visitVarInsn(ALOAD, 0);
        constructor.visitMethodInsn(INVOKESPECIAL, "java/lang/Object", "<init>", "()V", false);
        constructor.visitInsn(RETURN);
        constructor.visitMaxs(1, 1);
        constructor.visitEnd();

        // Build our methods (simulating Torque codegen output)
        build_add(cw);
        build_factorial(cw);

        cw.visitEnd();

        // Get the bytecode
        byte[] bytecode = cw.toByteArray();
        System.out.println("Generated " + bytecode.length + " bytes of bytecode");

        // Load the class dynamically
        ClassLoader loader = new ClassLoader() {
            @Override
            protected Class<?> findClass(String name) {
                return defineClass(name, bytecode, 0, bytecode.length);
            }
        };

        Class<?> generatedClass = loader.loadClass("e2e.GeneratedMath");
        System.out.println("Loaded class: " + generatedClass.getName());

        // Test the add method
        java.lang.reflect.Method addMethod = generatedClass.getMethod("add", int.class, int.class);
        int sum = (Integer) addMethod.invoke(null, 3, 4);
        System.out.println("add(3, 4) = " + sum);
        assert sum == 7 : "Expected 7, got " + sum;

        // Test the factorial method
        java.lang.reflect.Method factorialMethod = generatedClass.getMethod("factorial", int.class);
        int fact5 = (Integer) factorialMethod.invoke(null, 5);
        System.out.println("factorial(5) = " + fact5);
        assert fact5 == 120 : "Expected 120, got " + fact5;

        int fact10 = (Integer) factorialMethod.invoke(null, 10);
        System.out.println("factorial(10) = " + fact10);
        assert fact10 == 3628800 : "Expected 3628800, got " + fact10;

        System.out.println("\n=== All tests passed! ===");
    }
}
