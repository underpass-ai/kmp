/* WebGL camera adapter: fit the complete layered volume. */
KMP_APP.camera = (() => {
  const { THREE } = KMP_THREE;

  // Fit the actual layered volume, including labels, to any viewport aspect ratio.
  function fittedCamera(layout, mode, aspect, xScale = 1) {
    aspect = Math.max(0.1, aspect);
    const corners = layout.planes.flatMap((p) =>
      [-540, 510].flatMap((x) =>
        [-165, 185].flatMap((y) =>
          [-12, 12].map((z) => new THREE.Vector3(x * xScale, p.y + y, p.z + z)),
        ),
      ),
    );
    if (!corners.length)
      corners.push(
        new THREE.Vector3(-540, -165, 0),
        new THREE.Vector3(510, 185, 0),
      );
    if (mode === "flat") {
      const extent = new THREE.Box3().setFromPoints(corners),
        size = extent.getSize(new THREE.Vector3());
      const height = Math.max(size.y, size.x / aspect) * 1.12;
      const camera = new THREE.OrthographicCamera(
        (-height * aspect) / 2,
        (height * aspect) / 2,
        height / 2,
        -height / 2,
        1,
        10000,
      );
      camera.position.set(0, 0, 1600);
      camera.lookAt(0, 0, 0);
      return { camera, flatSize: height };
    }
    const camera = new THREE.PerspectiveCamera(38, aspect, 1, 10000),
      direction = new THREE.Vector3(0.3, 0.45, 1).normalize();
    camera.position.copy(direction);
    camera.lookAt(0, 0, 0);
    const inverse = camera.quaternion.clone().invert(),
      tanY = Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)),
      tanX = tanY * aspect;
    const distance = Math.max(
      350,
      ...corners.map((point) => {
        const v = point.clone().applyQuaternion(inverse);
        return (
          v.z + Math.max(Math.abs(v.x) / tanX, Math.abs(v.y) / tanY) * 1.12
        );
      }),
    );
    camera.position.copy(direction.multiplyScalar(distance));
    camera.lookAt(0, 0, 0);
    camera.updateMatrixWorld();
    return { camera };
  }

  return { fittedCamera };
})();
