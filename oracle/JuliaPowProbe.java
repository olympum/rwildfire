// SPDX-License-Identifier: GPL-3.0-or-later
import org.jwildfire.base.mathlib.FastMathLibImpl;

public final class JuliaPowProbe {
  public static void main(String[] args) {
    FastMathLibImpl math = new FastMathLibImpl();
    for (int power = -9; power <= 9; power++) {
      if (power == 0) continue;
      double exponent = (1.0 / power - 1.0) * 0.5;
      for (int i = -1074; i <= 1023; i++) {
        double offset = ((i & 31) + 1) / 32.0;
        for (double base : new double[]{Math.scalb(1.0 + offset, i),
            Math.nextDown(Math.scalb(1.0, i)), Math.nextUp(Math.scalb(1.0, i))}) {
          System.out.printf("%016x %016x %016x%n", Double.doubleToLongBits(base),
              Double.doubleToLongBits(exponent), Double.doubleToLongBits(math.pow(base, exponent)));
        }
      }
    }
  }
}
