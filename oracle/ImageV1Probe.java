// SPDX-License-Identifier: GPL-3.0-or-later
import java.awt.image.BufferedImage;
import java.io.BufferedInputStream;
import java.io.DataInputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import javax.imageio.ImageIO;
import org.jwildfire.base.Prefs;
import org.jwildfire.create.tina.base.Flame;
import org.jwildfire.create.tina.base.raster.RasterFloatInt;
import org.jwildfire.create.tina.io.FlameReader;
import org.jwildfire.create.tina.random.MarsagliaRandomGenerator;
import org.jwildfire.create.tina.render.GammaCorrectedRGBPoint;
import org.jwildfire.create.tina.render.GammaCorrectionFilter;
import org.jwildfire.create.tina.render.LogDensityFilter;
import org.jwildfire.create.tina.render.LogDensityPoint;
import org.jwildfire.create.tina.render.backdrop.FlameBackgroundRenderContext;

public class ImageV1Probe {
  private static final class SharedRaster extends RasterFloatInt {
    void read(Flame flame, int width, int height, int quality, DataInputStream in) throws IOException {
      allocRaster(flame, width, height, 1, quality);
      for (int y = 0; y < height; y++) {
        for (int x = 0; x < width; x++) {
          count[x][y] = in.readInt();
          red[x][y] = in.readFloat();
          green[x][y] = in.readFloat();
          blue[x][y] = in.readFloat();
        }
      }
      if (in.read() != -1) throw new IOException("extra raster bytes");
    }
  }

  public static void main(String[] args) throws Exception {
    Flame flame = new FlameReader(Prefs.getPrefs()).readFlames(args[0]).get(0);
    int width = flame.getWidth() * 2;
    int height = flame.getHeight() * 2;
    flame.setPixelsPerUnitScale(2.0);
    flame.setSpatialOversampling(1);
    String mode = args[3];
    boolean filtered = mode.equals("filtered") || mode.equals("omit");
    if (!filtered && !mode.equals("unfiltered")) throw new IllegalArgumentException("unknown mode");
    if (!filtered) flame.setSpatialFilterRadius(0.0);
    flame.setAntialiasAmount(0.0);
    LogDensityFilter density = new LogDensityFilter(flame, new MarsagliaRandomGenerator());
    int border = density.getBorderWidth();
    if (filtered && (!flame.getSpatialFilterKernel().name().equals("MITCHELL_SMOOTH")
        || flame.getSpatialFilterRadius() != 1.0 || border != 2)) {
      throw new AssertionError("expected original TINA0010 Mitchell Smooth radius 1, border 2");
    }
    int rasterWidth = width + 2 * border;
    int rasterHeight = height + 2 * border;
    SharedRaster raster = new SharedRaster();
    try (DataInputStream in = new DataInputStream(new BufferedInputStream(new FileInputStream(args[1])))) {
      if (in.readInt() != width || in.readInt() != height) throw new IOException("raster size differs from flame");
      int quality = in.readInt();
      if (quality <= 0) throw new IOException("invalid raster quality");
      flame.setSampleDensity(quality);
      raster.read(flame, rasterWidth, rasterHeight, quality, in);
    }
    density.setRaster(raster, rasterWidth, rasterHeight, width, height);
    GammaCorrectionFilter gamma = new GammaCorrectionFilter(flame, false, width, height);
    FlameBackgroundRenderContext background = new FlameBackgroundRenderContext(flame, 0);
    LogDensityPoint logPoint = new LogDensityPoint(0);
    GammaCorrectedRGBPoint rgb = new GammaCorrectedRGBPoint();
    BufferedImage image = new BufferedImage(width, height, BufferedImage.TYPE_INT_RGB);
    for (int y = 0; y < height; y++) {
      for (int x = 0; x < width; x++) {
        if (mode.equals("filtered")) density.transformPoint(background, logPoint, x, y);
        else density.transformPointSimple(background, logPoint, x, y);
        gamma.transformPoint(logPoint, rgb, x, y);
        image.setRGB(x, y, (rgb.red << 16) | (rgb.green << 8) | rgb.blue);
      }
    }
    if (!ImageIO.write(image, "png", new File(args[2]))) throw new IOException("no PNG writer");
  }
}
