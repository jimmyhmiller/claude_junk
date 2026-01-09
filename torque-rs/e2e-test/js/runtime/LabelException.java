package js.runtime;

/**
 * Exception used to implement Torque's goto/labels mechanism.
 * When a goto is executed, this exception is thrown and caught
 * by the corresponding label handler.
 */
public class LabelException extends RuntimeException {
    private final String label;

    public LabelException(String label) {
        super("Goto label: " + label);
        this.label = label;
    }

    public String getLabel() {
        return label;
    }
}
