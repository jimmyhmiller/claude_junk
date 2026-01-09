package js.builtins;

import org.objectweb.asm.*;
import org.objectweb.asm.commons.*;
import static org.objectweb.asm.Opcodes.*;

/**
 * Generated from Torque source.
 * This class builds JVM bytecode for JavaScript builtins.
 */
public class array {
    /**
     * Generates bytecode for SmiAdd builtin.
     * @param cw The ClassWriter to emit bytecode to
     */
    public static void build_SmiAdd(ClassWriter cw) {
        MethodVisitor mv = cw.visitMethod(
            ACC_PUBLIC | ACC_STATIC,
            "SmiAdd",
            "(Ljs/runtime/Smi;Ljs/runtime/Smi;)Ljs/runtime/Smi;",
            null,
            null);
        mv.visitCode();

        mv.visitVarInsn(ALOAD, 0);  // a
        mv.visitMethodInsn(INVOKEVIRTUAL, "js/runtime/Smi", "intValue", "()I", false);
        mv.visitVarInsn(ALOAD, 1);  // b
        mv.visitMethodInsn(INVOKEVIRTUAL, "js/runtime/Smi", "intValue", "()I", false);
        mv.visitInsn(IADD);
        mv.visitTypeInsn(NEW, "js/runtime/Smi");
        mv.visitInsn(DUP_X1);
        mv.visitInsn(SWAP);
        mv.visitMethodInsn(INVOKESPECIAL, "js/runtime/Smi", "<init>", "(I)V", false);
        mv.visitInsn(ARETURN);

        mv.visitMaxs(-1, -1);  // Auto-computed
        mv.visitEnd();
    }

    /**
     * Generates bytecode for ArrayPush builtin.
     * @param cw The ClassWriter to emit bytecode to
     */
    public static void build_ArrayPush(ClassWriter cw) {
        MethodVisitor mv = cw.visitMethod(
            ACC_PUBLIC | ACC_STATIC,
            "ArrayPush",
            "(Ljs/runtime/Context;Ljava/lang/Object;Ljs/runtime/Context;Ljs/runtime/JSArray;Ljava/lang/Object;)D",
            null,
            null);
        mv.visitCode();

        // let length = ...
        mv.visitVarInsn(ALOAD, 3);  // receiver
        mv.visitFieldInsn(GETFIELD, "js/runtime/JSObject", "length", "Ljava/lang/Object;");
        mv.visitVarInsn(ASTORE, 5);
        Label else_1 = new Label();
        Label endif_2 = new Label();
        mv.visitVarInsn(ALOAD, 5);  // length
        mv.visitInsn(ICONST_0);
        Label cmp_true_3 = new Label();
        Label cmp_end_4 = new Label();
        mv.visitJumpInsn(IF_ICMPEQ, cmp_true_3);
        mv.visitInsn(ICONST_0);
        mv.visitJumpInsn(GOTO, cmp_end_4);
        mv.visitLabel(cmp_true_3);
        mv.visitInsn(ICONST_1);
        mv.visitLabel(cmp_end_4);
        mv.visitJumpInsn(IFEQ, else_1);
        mv.visitInsn(ICONST_0);
        mv.visitInsn(DRETURN);
        mv.visitJumpInsn(GOTO, endif_2);
        mv.visitLabel(else_1);
        mv.visitLabel(endif_2);
        mv.visitVarInsn(ALOAD, 5);  // length
        mv.visitInsn(ICONST_1);
        mv.visitInsn(IADD);
        mv.visitInsn(DRETURN);

        mv.visitMaxs(-1, -1);  // Auto-computed
        mv.visitEnd();
    }

    /**
     * Generates bytecode for GetElement builtin.
     * @param cw The ClassWriter to emit bytecode to
     */
    public static void build_GetElement(ClassWriter cw) {
        MethodVisitor mv = cw.visitMethod(
            ACC_PUBLIC | ACC_STATIC,
            "GetElement",
            "(Ljs/runtime/JSArray;Ljs/runtime/Smi;)Ljava/lang/Object;",
            null,
            null);
        mv.visitCode();

        Label NotFound_5 = new Label();
        Label else_6 = new Label();
        Label endif_7 = new Label();
        mv.visitVarInsn(ALOAD, 1);  // index
        mv.visitMethodInsn(INVOKEVIRTUAL, "js/runtime/Smi", "intValue", "()I", false);
        mv.visitVarInsn(ALOAD, 0);  // array
        mv.visitFieldInsn(GETFIELD, "js/runtime/JSObject", "length", "Ljava/lang/Object;");
        Label cmp_true_8 = new Label();
        Label cmp_end_9 = new Label();
        mv.visitJumpInsn(IF_ICMPGE, cmp_true_8);
        mv.visitInsn(ICONST_0);
        mv.visitJumpInsn(GOTO, cmp_end_9);
        mv.visitLabel(cmp_true_8);
        mv.visitInsn(ICONST_1);
        mv.visitLabel(cmp_end_9);
        mv.visitJumpInsn(IFEQ, else_6);
        // goto NotFound - throw LabelException
        mv.visitTypeInsn(NEW, "js/runtime/LabelException");
        mv.visitInsn(DUP);
        mv.visitLdcInsn("NotFound");
        mv.visitMethodInsn(INVOKESPECIAL, "js/runtime/LabelException", "<init>", "(Ljava/lang/String;)V", false);
        mv.visitInsn(ATHROW);
        mv.visitJumpInsn(GOTO, endif_7);
        mv.visitLabel(else_6);
        mv.visitLabel(endif_7);
        mv.visitVarInsn(ALOAD, 0);  // array
        mv.visitFieldInsn(GETFIELD, "js/runtime/JSObject", "elements", "Ljava/lang/Object;");
        mv.visitVarInsn(ALOAD, 1);  // index
        mv.visitInsn(AALOAD);
        mv.visitInsn(ARETURN);

        mv.visitMaxs(-1, -1);  // Auto-computed
        mv.visitEnd();
    }

}

