/*
 * rwildfire parity oracle: primitives-v1
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Executes the pinned, unmodified upstream JWildfire classes. This probe does
 * not reimplement any formula: every value below is produced by upstream code
 * loaded from the pinned source tree.
 */
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

import org.jwildfire.create.tina.base.Layer;
import org.jwildfire.create.tina.base.XForm;
import org.jwildfire.create.tina.base.XYZPoint;
import org.jwildfire.create.tina.random.MarsagliaRandomGenerator;
import org.jwildfire.create.tina.variation.BlurFunc;
import org.jwildfire.create.tina.variation.FlameTransformationContext;
import org.jwildfire.create.tina.variation.LinearFunc;
import org.jwildfire.create.tina.variation.SinusoidalFunc;
import org.jwildfire.create.tina.variation.SphericalFunc;
import org.jwildfire.create.tina.variation.SwirlFunc;
import org.jwildfire.create.tina.variation.VariationFunc;

public final class PrimitivesV1Probe {
  private static final int SCHEMA_VERSION = 1;
  private static final String SUITE = "primitives-v1";
  private static final long SEED = 123456789L;
  private static final int RNG_VALUE_COUNT = 16;

  // Fixed, nontrivial probe point shared by every variation case.
  private static final double INPUT_X = 0.3;
  private static final double INPUT_Y = -0.7;
  private static final double INPUT_Z = 0.45;
  private static final double AMOUNT = 1.25;

  private static final class ProbeCase {
    final String id;
    final double[] values;

    ProbeCase(String id, double[] values) {
      this.id = id;
      this.values = values;
    }
  }

  public static void main(String[] args) {
    String commit = null;
    for (int i = 0; i < args.length; i++) {
      if ("--commit".equals(args[i]) && i + 1 < args.length) {
        commit = args[++i];
      }
      else {
        fail("unknown argument: " + args[i]);
      }
    }
    if (commit == null || !commit.matches("[0-9a-f]{40}")) {
      fail("--commit must be a full 40-character lowercase Git SHA");
    }

    List<ProbeCase> cases = new ArrayList<ProbeCase>();
    cases.add(new ProbeCase("rng/marsaglia/seed-" + SEED + "/first-" + RNG_VALUE_COUNT, rngValues()));

    // Deterministic and stochastic real upstream variations. Each case gets a
    // fresh seeded generator and the real lifecycle calls (initOnce, init).
    addVariation(cases, "linear", new LinearFunc());
    addVariation(cases, "sinusoidal", new SinusoidalFunc());
    addVariation(cases, "spherical", new SphericalFunc());
    addVariation(cases, "swirl", new SwirlFunc());
    addVariation(cases, "blur", new BlurFunc());

    Set<String> ids = new HashSet<String>();
    for (ProbeCase c : cases) {
      if (!ids.add(c.id)) {
        fail("duplicate case id: " + c.id);
      }
      if (c.values == null || c.values.length == 0) {
        fail("empty values for case: " + c.id);
      }
      for (double v : c.values) {
        if (Double.isNaN(v) || Double.isInfinite(v)) {
          fail("non-finite value for case: " + c.id);
        }
      }
    }

    StringBuilder out = new StringBuilder(4096);
    out.append("{\n");
    out.append("  \"schema_version\": ").append(SCHEMA_VERSION).append(",\n");
    out.append("  \"upstream_commit\": ");
    appendString(out, commit);
    out.append(",\n");
    out.append("  \"suite\": ");
    appendString(out, SUITE);
    out.append(",\n");
    out.append("  \"seed\": ").append(SEED).append(",\n");
    out.append("  \"cases\": [\n");
    for (int i = 0; i < cases.size(); i++) {
      ProbeCase c = cases.get(i);
      out.append("    {\"id\": ");
      appendString(out, c.id);
      out.append(", \"values\": [");
      for (int j = 0; j < c.values.length; j++) {
        if (j > 0) {
          out.append(", ");
        }
        out.append(Double.toString(c.values[j]));
      }
      out.append("]}");
      if (i + 1 < cases.size()) {
        out.append(",");
      }
      out.append("\n");
    }
    out.append("  ]\n");
    out.append("}\n");
    System.out.print(out.toString());
    System.out.flush();
  }

  private static double[] rngValues() {
    MarsagliaRandomGenerator rng = new MarsagliaRandomGenerator();
    rng.randomize(SEED);
    double[] values = new double[RNG_VALUE_COUNT];
    for (int i = 0; i < values.length; i++) {
      values[i] = rng.random();
    }
    return values;
  }

  private static void addVariation(List<ProbeCase> cases, String name, VariationFunc variation) {
    cases.add(new ProbeCase("variation/" + name + "/preserve-z-false", runVariation(variation, false)));
    cases.add(new ProbeCase("variation/" + name + "/preserve-z-true", runVariation(variation, true)));
  }

  private static double[] runVariation(VariationFunc variation, boolean preserveZ) {
    MarsagliaRandomGenerator rng = new MarsagliaRandomGenerator();
    rng.randomize(SEED);

    FlameTransformationContext context = new FlameTransformationContext(null, rng, 0, 0);
    context.setPreserveZCoordinate(preserveZ);

    Layer layer = new Layer();
    XForm xform = new XForm();
    variation.initOnce(context, layer, xform, AMOUNT);
    variation.init(context, layer, xform, AMOUNT);

    XYZPoint affine = new XYZPoint();
    affine.x = INPUT_X;
    affine.y = INPUT_Y;
    affine.z = INPUT_Z;
    XYZPoint result = new XYZPoint();

    variation.transform(context, xform, affine, result, AMOUNT);
    return new double[] { result.x, result.y, result.z };
  }

  private static void appendString(StringBuilder out, String value) {
    out.append('"');
    for (int i = 0; i < value.length(); i++) {
      char ch = value.charAt(i);
      if (ch == '"' || ch == '\\') {
        out.append('\\').append(ch);
      }
      else if (ch < 0x20) {
        out.append(String.format("\\u%04x", (int) ch));
      }
      else {
        out.append(ch);
      }
    }
    out.append('"');
  }

  private static void fail(String message) {
    System.err.println("primitives-v1 probe: " + message);
    System.exit(2);
  }
}
