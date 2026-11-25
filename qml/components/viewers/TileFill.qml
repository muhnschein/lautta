// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6

ShaderEffect {
    id: fill

    property Item tile
    readonly property real side: tile ? tile.width : 0

    property variant source: ShaderEffectSource {
        sourceItem: fill.tile
        hideSource: true
        mipmap: true
        textureSize: Qt.size(256, 256)
    }
    property size extent: Qt.size(width / Math.max(1, side), height / Math.max(1, side))
    property real pixels: side

    fragmentShader: "
        varying highp vec2 qt_TexCoord0;
        uniform sampler2D source;
        uniform lowp float qt_Opacity;
        uniform highp vec2 extent;
        uniform highp float pixels;
        void main() {
            highp vec2 t = (qt_TexCoord0 - 0.5) * extent + 0.5;
            highp vec2 beyond = max(abs(t - 0.5) - 0.5, 0.0);
            highp float d = beyond.x + beyond.y;
            // Radius in tile sides: along the edge grows with distance, inward stays a thin band.
            highp vec2 r = mix(vec2(0.006 + 0.35 * d), vec2(0.006 + min(0.35 * d, 0.02)), step(0.000001, beyond));
            highp vec2 c = clamp(t, 0.0, 1.0);
            mediump float bias = log2(max(r.x, r.y) * pixels) - 0.5;
            mediump vec4 sum = vec4(0.0);
            mediump float total = 0.0;
            for (int i = -2; i <= 2; ++i) {
                for (int j = -2; j <= 2; ++j) {
                    highp vec2 o = vec2(float(i), float(j));
                    mediump float w = exp(-0.5 * dot(o, o));
                    sum += w * texture2D(source, clamp(c + o * r, 0.0, 1.0), bias);
                    total += w;
                }
            }
            gl_FragColor = sum / total * qt_Opacity;
        }"
}
