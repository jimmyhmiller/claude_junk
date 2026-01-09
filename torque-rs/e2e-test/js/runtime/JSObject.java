package js.runtime;

/**
 * Stub runtime class for testing Torque codegen.
 * Base class for all JavaScript objects.
 */
public class JSObject {
    public Object length;
    public Object[] elements;

    public JSObject() {
        this.length = new Smi(0);
        this.elements = new Object[0];
    }

    public JSObject(int length) {
        this.length = new Smi(length);
        this.elements = new Object[length];
    }
}
