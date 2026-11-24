import java.util.Random;
import org.jwildfire.base.mathlib.FastMathLibImpl;

public final class TrigProbe {
  private static final FastMathLibImpl MATH = new FastMathLibImpl();

  private static void sample(double x) {
    System.out.printf("%016x %016x %016x %016x%n", Double.doubleToLongBits(x),
        Double.doubleToLongBits(MATH.sin(x)), Double.doubleToLongBits(MATH.cos(x)),
        Double.doubleToLongBits(MATH.tan(x)));
  }

  public static void main(String[] args) {
    for (double x : new double[]{0.0, Math.PI / 2, 3 * Math.PI / 2, 12739, 1600,
        Math.scalb(2 * Math.PI, 20), Double.MAX_VALUE}) {
      for (double a : new double[]{Math.nextDown(x), x, Math.nextUp(x)}) {
        sample(a);
        sample(-a);
      }
    }
    sample(Double.NaN);
    Random random = new Random(741103597L);
    for (int i = 0; i < 2048; i++) {
      sample(Math.scalb(random.nextDouble() - 0.5, i - 1024));
    }
  }
}
