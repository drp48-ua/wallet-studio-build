# Native shader resource observer

This diagnostic investigates a different contract from the previously measured
PKImage bridge and emitter presets. It invokes the genuine
`PKLiveRenderedShaderSet.initWithDataAccessor:suffix:` in its own app process,
with an empty synthetic accessor and an empty suffix. A runtime subclass of
the genuine PKDataAccessor records three resource selectors declared in the
public iOS 17 runtime headers. Every method signature is verified before use.

Revision v3 also handles the measured NSSet collection and recognizes genuine
PKBundleResourceDescriptor objects through their verified name/extension
getters. Other object types remain rejected; descriptions are never exported.

Revision v4 uses the four measured metal resource names to create an empty
synthetic pass.json and four comment-only source files in this app's own Caches
directory. It invokes the genuine PKFileDataAccessor initializer after checking
its signature, then checks exact preservation of the known comment by all four
native shader getters. It does not compile or execute a shader, and a failed
directory initializer remains unknown. No signed payment pass is fabricated.

Revision v5 constructs a synthetic generic PKPass with documented public keys
and an own bundle. It compares native reference/recording-dictionary outcomes,
records only sanitized lookup keys, and reads genuine live-render getters after
checking their ABI. No private keys, render-type values or setters are supplied.
The observer's trace is a partial record of dictionary lookups, not all nested
parsing or copied-dictionary accesses. A generic pass result does not establish
activation for a bank payment pass or signature admission by Wallet.

Revision v6 binds the genuine own-file accessor through PKObject's verified
setter and tests only the liveRenderedBackground boolean read by the prior
native trace. In simulator this yields native render type 1 without assigning
an enum and preserves the four sources through the pass model.

Revision v7 presents PKLiveRenderedCardFaceView for that own synthetic pass.
It supplies an owned fragment snippet using SceneKit's documented u_time and
fragment output. This is a candidate shader contract, not a presumed match to
the Wallet renderer. A calibrated UIKit capture samples six center pixels.
Missing phases do not identify whether GPU capture, shader compilation,
consumer selection or clock activation failed. Even positive phases in the
own app would not prove real Wallet acceptance or a valid signed payment pass.

Revision v8 uses the genuine PKPaymentPass class for the same synthetic metadata
and own sources after the generic model's view initializer returned nil. This
constructs only an in-memory diagnostic object, not a valid payment card, secure
element credential or signed issuer pass. The native constructor/type/source
checks gate the test; no issuer fields are fabricated and no bank data is used.

Revision v9 adds a solid white owned PNG through genuine PKImage and
PKPassLiveRenderedImageSet. It obtains imageSetType from the real class method,
binds it through the verified native setter and requires exact identity through
liveRenderedImageSet. This isolates a missing diffuse input without choosing an
image-set enum, fabricating an archive or changing any card resource.

The observer supplies nil data, not shader source, bank metadata or an archive.
It does not read Wallet, open a pass library, connect to USB, modify a bank card,
or install an entitlement. Resource names are released only if the reference
constructor returns an object, observable outcomes agree, the dispatch control
works, and the trace passes bounded privacy filters. Nil, exceptions and missing
signatures remain unknown. Matching getters do not prove full behavioral
equivalence of the accessor implementations.

The reference is the native empty PKDataAccessor, not an assumed bank bundle.
An empty suffix is a diagnostic argument; no claim is made that it represents
the suffix selected by a real payment card. The current runtime may differ from
the old declarations. A result with no requests does not prove that shaders are
unsupported in every configuration.

Even a successful resource trace is discovery only. It does not establish shader
acceptance, a frame clock, activation in Wallet, or GIF playback. Shader content,
pass selection and the actual renderer must be demonstrated separately before
any proposed card write.

Source declarations:

- https://github.com/MTACS/iOS-17-Runtime-Headers/blob/d1d960dfaa4107765dd7fcf891e4967c0930d5fd/PrivateFrameworks/PassKitCore.framework/PKLiveRenderedShaderSet.h
- https://github.com/MTACS/iOS-17-Runtime-Headers/blob/d1d960dfaa4107765dd7fcf891e4967c0930d5fd/PrivateFrameworks/PassKitCore.framework/PKDataAccessor.h

Build and run on macOS with Xcode:

```sh
bash build.sh
python3 run-simulator.py
```

The workflow produces a strictly validated simulator JSON and an unsigned arm64
device IPA. Signing and installation require the user's existing personal
workflow. This is a separate application and writes only `shader-report.json`
in its own Documents directory.
