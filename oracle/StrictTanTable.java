// SPDX-License-Identifier: GPL-3.0-or-later
// Table layout and derivatives adapted from JWildfire 9.20 FastMathLibImpl
// (Andreas Maschke, LGPL-2.1-or-later) and Jafama (Jeff Hain, Apache-2.0).
// Regenerate crates/cpu/src/strict_tan_table.in with JDK 8. See NOTICE.md.
public final class StrictTanTable {
  private static String bits(double value) {
    return String.format("0x%016x", Double.doubleToRawLongBits(value));
  }

  public static void main(String[] args) {
    int virtualTabsSize = 4097;
    // FastMathLibImpl.PIO2_HI/PIO2_LO: a split pi/2, not the rounded value.
    double pio2Hi = Double.longBitsToDouble(0x3ff921fb54400000L);
    double pio2Lo = Double.longBitsToDouble(0x3dd0b4611a626331L);
    double maxValueForTabs = Math.toRadians(77.0);
    int tabsSize = (int) (maxValueForTabs / 1.5707963267948966 * (double) (virtualTabsSize - 1)) + 1;
    double deltaHi = pio2Hi / (double) (virtualTabsSize - 1);
    double deltaLo = pio2Lo / (double) (virtualTabsSize - 1);
    double indexer = 1.0 / (deltaHi + deltaLo);
    double maxValueForIntModulo = 4194303.0 / indexer * 0.99;

    System.out.println("[");
    for (int i = 0; i < tabsSize; i++) {
      double x = (double) i * deltaHi + (double) i * deltaLo;
      double tan = StrictMath.tan(x);
      double cos = StrictMath.cos(x);
      double sin = StrictMath.sin(x);
      double inv = 1.0 / cos;
      double inv2 = inv * inv;
      double inv3 = inv2 * inv;
      double inv4 = inv2 * inv2;
      double inv5 = inv3 * inv2;
      double der1DivF1 = inv2;
      double der2DivF2 = 2.0 * sin * inv3 * 0.5;
      double der3DivF3 = 2.0 * (1.0 + 2.0 * sin * sin) * inv4 * 0.16666666666666666;
      double der4DivF4 = 8.0 * sin * (2.0 + sin * sin) * inv5 * 0.041666666666666664;
      System.out.printf("    (%s, %s, %s, %s, %s),%n",
          bits(tan), bits(der1DivF1), bits(der2DivF2), bits(der3DivF3), bits(der4DivF4));
    }
    System.out.println("]");
    System.err.printf("virtual=%d tabs=%d deltaHi=%s deltaLo=%s indexer=%s maxIntModulo=%s%n",
        virtualTabsSize, tabsSize, bits(deltaHi), bits(deltaLo), bits(indexer), bits(maxValueForIntModulo));
  }
}
