package js.runtime;

/**
 * Stub runtime class for testing Torque codegen.
 * Represents a JavaScript Array object.
 */
public class JSArray extends JSObject {
    public JSArray() {
        super();
    }

    public JSArray(int length) {
        super(length);
    }

    public JSArray(Object... elements) {
        this.elements = elements;
        this.length = new Smi(elements.length);
    }
}
