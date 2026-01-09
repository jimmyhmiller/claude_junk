package js.runtime;

/**
 * Stub runtime class for testing Torque codegen.
 * In a real implementation, this would be a tagged integer.
 */
public class Smi extends Number {
    private final int value;

    public Smi(int value) {
        this.value = value;
    }

    public int getValue() {
        return value;
    }

    @Override
    public int intValue() { return value; }

    @Override
    public long longValue() { return value; }

    @Override
    public float floatValue() { return value; }

    @Override
    public double doubleValue() { return value; }

    @Override
    public String toString() {
        return "Smi(" + value + ")";
    }
}
