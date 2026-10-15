import re

def exact(obj,keys):
    if type(obj) is not dict or set(obj)!=set(keys.split()): raise ValueError('Unexpected report keys')

def boolean(value):
    if type(value) is not bool: raise ValueError('Expected bool')

def native(obj):
    exact(obj,'status object_returned exception getter_lengths')
    if obj['status'] not in ('observed','unknown'): raise ValueError('Invalid native status')
    boolean(obj['object_returned']); boolean(obj['exception'])
    lengths=obj['getter_lengths']
    if type(lengths) is not list or len(lengths) not in (0,4): raise ValueError('Invalid getters')
    if any(type(x) is not int or not 0<=x<=1000000 for x in lengths): raise ValueError('Invalid lengths')
    if obj['status']=='observed' and (not obj['object_returned'] or obj['exception'] or len(lengths)!=4): raise ValueError('Invalid positive result')

def validate(data):
    if data.get('revision') not in ('shader_resources_v4','shader_resources_v5','shader_resources_v6','shader_resources_v7','shader_resources_v8','shader_resources_v9','shader_resources_v10','shader_resources_v11','shader_resources_v12','shader_resources_v13','shader_resources_v14'): raise ValueError('Invalid revision')
    has_renderer=data['revision'] in ('shader_resources_v7','shader_resources_v8','shader_resources_v9','shader_resources_v10','shader_resources_v11','shader_resources_v12','shader_resources_v13','shader_resources_v14')
    exact(data,'schema revision scope Wallet_animation_proven runtime_iOS environment framework_loaded probe file_loader'+(' pass_probe' if data['revision']!='shader_resources_v4' else '')+(' renderer_probe' if has_renderer else '')+(' constructor_probe' if data['revision'] in ('shader_resources_v13','shader_resources_v14') else ''))
    if type(data['schema']) is not int or data['schema']!=1: raise ValueError('Invalid schema')
    if data['scope']!='synthetic_accessors_and_own_files_only': raise ValueError('Invalid scope')
    if data['Wallet_animation_proven'] is not False: raise ValueError('Wallet playback unproven')
    if type(data['runtime_iOS']) is not str or not re.fullmatch(r'\d{1,2}(?:\.\d{1,2}){1,2}',data['runtime_iOS']): raise ValueError('Invalid runtime')
    if data['environment'] not in ('simulator','device_app_own_sandbox'): raise ValueError('Invalid environment')
    if data['revision'] in ('shader_resources_v13','shader_resources_v14'):
        c=data['constructor_probe']; exact(c,'status reason image_origin_verified code_hex'+(' selector_calls' if data['revision']=='shader_resources_v14' else ''))
        code_length=8192 if data['revision']=='shader_resources_v14' else 1024
        boolean(c['image_origin_verified'])
        if c['status'] not in ('unknown','observed'): raise ValueError('Invalid constructor status')
        if c['reason'] not in {'method_unavailable','image_origin_unverified','own_code_read_unavailable','own_constructor_code_observed'}: raise ValueError('Invalid constructor reason')
        if type(c['code_hex']) is not str: raise ValueError('Unsafe code representation')
        if c['status']=='observed':
            if not c['image_origin_verified'] or c['reason']!='own_constructor_code_observed' or not re.fullmatch(r'[0-9a-f]{'+str(code_length)+'}',c['code_hex']): raise ValueError('Unestablished constructor code')
        elif c['code_hex'] or c['reason']=='own_constructor_code_observed': raise ValueError('Unknown constructor code')
        if data['revision']=='shader_resources_v14':
            calls=c['selector_calls']
            if type(calls) is not list or len(calls)>128: raise ValueError('Unsafe call count')
            previous=-1
            for call in calls:
                exact(call,'offset selector')
                if type(call['offset']) is not int or not 0<=call['offset']<4096 or call['offset']%4 or call['offset']<=previous: raise ValueError('Unsafe call offset')
                previous=call['offset']
                if type(call['selector']) is not str or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_:]{0,127}',call['selector']): raise ValueError('Unsafe selector')
            if calls and c['status']!='observed': raise ValueError('Unknown selector calls')

    boolean(data['framework_loaded'])
    p=data['probe']
    exact(p,'status reason reference observer requests calibration parity trace_rejected trace_truncated rejection_kinds')
    if p['status'] not in ('observed','unknown'): raise ValueError('Invalid status')
    reasons={'runtime_unavailable','signature_unavailable','observer_unavailable','accessor_unavailable','accessor_exception','native_resource_requests_observed','no_resource_requests_observed','control_not_established'}
    if p['reason'] not in reasons: raise ValueError('Invalid reason')
    for k in ('calibration','parity','trace_rejected','trace_truncated'): boolean(p[k])
    kinds=p['rejection_kinds']
    allowed={'nonstring_resource','empty_string','path_string','other_string','nil_group','set_group','dictionary_group','other_group'}
    if type(kinds) is not list or len(kinds)>8 or any(type(k) is not str or k not in allowed for k in kinds) or len(set(kinds))!=len(kinds): raise ValueError('Invalid rejection categories')
    for k in ('reference','observer'):
        if p[k]: native(p[k])
        elif type(p[k]) is not dict: raise ValueError('Invalid control')
    if type(p['requests']) is not list or len(p['requests'])>32: raise ValueError('Unbounded trace')
    for row in p['requests']:
        exact(row,'method name extension')
        if row['method'] not in ('resource','named_resource','resources'): raise ValueError('Invalid resource method')
        for k in ('name','extension'):
            value=row[k]
            if value is not None and (type(value) is not str or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_.-]{0,95}',value) or re.search(r'[0-9a-fA-F]{24,}',value)): raise ValueError('Private or unexpected name')
    if p['status']=='observed':
        if not data['framework_loaded'] or not p['calibration'] or not p['parity'] or p['trace_rejected'] or p['trace_truncated']: raise ValueError('Unestablished control')
        if p['reference']!=p['observer'] or p['reference'].get('status')!='observed': raise ValueError('Invalid parity')
        if p['reason'] != ('native_resource_requests_observed' if p['requests'] else 'no_resource_requests_observed'): raise ValueError('Invalid trace interpretation')
    elif p['requests']: raise ValueError('Unknown cannot export resource names')
    f=data['file_loader']
    exact(f,'status reason native_accessor_created source_preserved')
    boolean(f['native_accessor_created']); boolean(f['source_preserved'])
    if f['status'] not in ('observed','unknown'): raise ValueError('Invalid loader status')
    reasons={'discovery_unavailable','file_accessor_signature_unavailable','synthetic_fixture_unavailable','file_accessor_rejected','shader_set_rejected','synthetic_source_preserved','synthetic_source_not_preserved','file_accessor_exception'}
    if f['reason'] not in reasons: raise ValueError('Invalid loader reason')
    if f['source_preserved'] and (f['status']!='observed' or not f['native_accessor_created'] or f['reason']!='synthetic_source_preserved' or p['status']!='observed'): raise ValueError('Unestablished loader success')
    if data['revision']!='shader_resources_v4':
        q=data['pass_probe']
        extra=' bound_reference bound_observer bound_parity live_reference live_observer live_parity' if data['revision']!='shader_resources_v5' else ''
        exact(q,'status reason reference observer parity metadata_lookup_keys trace_rejected trace_truncated'+extra)
        if q['status'] not in ('observed','unknown'): raise ValueError('Invalid pass status')
        reasons={'loader_control_unavailable','pass_signature_unavailable','pass_fixture_unavailable','synthetic_generic_pass_observed','pass_control_not_established','pass_fixture_exception'}
        if q['reason'] not in reasons: raise ValueError('Invalid pass reason')
        for k in ('parity','trace_rejected','trace_truncated'): boolean(q[k])
        controls=['reference','observer']
        if data['revision']!='shader_resources_v5':
            controls+=['bound_reference','bound_observer','live_reference','live_observer']
            for k in ('bound_parity','live_parity'): boolean(q[k])
        for k in controls:
            v=q[k]
            if v=={}: continue
            exact(v,'status pass_created exception live_background live_render_type shader_object_returned shader_source_preserved')
            if v['status'] not in ('observed','unknown'): raise ValueError('Invalid native pass status')
            for field in ('pass_created','exception','live_background','shader_object_returned','shader_source_preserved'): boolean(v[field])
            if type(v['live_render_type']) is not int or not 0<=v['live_render_type']<=1024: raise ValueError('Invalid observed render type')
            if v['status']=='observed' and (not v['pass_created'] or v['exception']): raise ValueError('Unestablished pass control')
            if v['shader_source_preserved'] and not v['shader_object_returned']: raise ValueError('False pass shader claim')
        keys=q['metadata_lookup_keys']
        if type(keys) is not list or len(keys)>128 or any(type(k) is not str or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]{0,63}',k) for k in keys): raise ValueError('Unsafe metadata trace')
        if q['status']=='observed':
            if not f['source_preserved'] or not q['parity'] or q['trace_rejected'] or q['trace_truncated'] or q['reference']!=q['observer'] or q['reference'].get('status')!='observed': raise ValueError('Unestablished pass observation')
        elif keys: raise ValueError('Unknown cannot export metadata keys')
    if has_renderer:
        r=data['renderer_probe']
        has_material=data['revision'] in ('shader_resources_v9','shader_resources_v10','shader_resources_v11','shader_resources_v12','shader_resources_v13','shader_resources_v14')
        exact(r,'status reason view_created shader_source_exact capture_control view_kinds samples_rgb three_phases_observed'+(' synthetic_model' if data['revision']!='shader_resources_v7' else '')+(' material_bound native_image_set_type' if has_material else '')+(' initializer_names' if data['revision'] in ('shader_resources_v10','shader_resources_v11','shader_resources_v12','shader_resources_v13','shader_resources_v14') else '')+(' admission_probe' if data['revision'] in ('shader_resources_v11','shader_resources_v12','shader_resources_v13','shader_resources_v14') else '')+(' profile_probe' if data['revision'] in ('shader_resources_v12','shader_resources_v13','shader_resources_v14') else ''))
        if data['revision']!='shader_resources_v7' and r['synthetic_model']!='PKPaymentPass': raise ValueError('Unexpected synthetic model')
        if has_material:
            boolean(r['material_bound'])
            if type(r['native_image_set_type']) is not int or not 0<=r['native_image_set_type']<=1024: raise ValueError('Invalid native image set type')
            if r['status']=='observed' and not r['material_bound']: raise ValueError('Unestablished material control')
        if data['revision'] in ('shader_resources_v10','shader_resources_v11','shader_resources_v12','shader_resources_v13','shader_resources_v14'):
            names=r['initializer_names']
            if type(names) is not list or len(names)>32 or any(type(n) is not str or not re.fullmatch(r'init[A-Za-z0-9_:]{0,124}',n) for n in names): raise ValueError('Unsafe initializer names')
        if data['revision'] in ('shader_resources_v12','shader_resources_v13','shader_resources_v14'):
            profile=r['profile_probe']
            exact(profile,'status reason reference_present constructed parity bound metadata_lookup_keys trace_rejected trace_truncated reference_snapshot observer_snapshot')
            if profile['status'] not in ('unknown','observed'): raise ValueError('Invalid profile status')
            if profile['reason'] not in {'profile_control_unavailable','reference_profile_already_present','profile_construction_not_established','profile_binding_not_established','synthetic_profile_bound','profile_exception'}: raise ValueError('Invalid profile reason')
            for key in ('reference_present','constructed','parity','bound','trace_rejected','trace_truncated'): boolean(profile[key])
            for key in ('reference_snapshot','observer_snapshot'):
                snapshot=profile[key]
                if snapshot=={}: continue
                exact(snapshot,'type passStyle layoutMode backgroundColorPresent foregroundColorPresent labelColorPresent')
                for scalar in ('type','passStyle','layoutMode'):
                    if type(snapshot[scalar]) is not int or not -1<=snapshot[scalar]<=1024: raise ValueError('Unsafe profile scalar')
                for flag in ('backgroundColorPresent','foregroundColorPresent','labelColorPresent'): boolean(snapshot[flag])
            keys=profile['metadata_lookup_keys']
            if type(keys) is not list or len(keys)>128 or any(type(k) is not str or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]{0,63}',k) for k in keys): raise ValueError('Unsafe profile keys')
            if profile['parity'] and (not profile['constructed'] or not profile['reference_snapshot'] or profile['reference_snapshot']!=profile['observer_snapshot'] or profile['trace_rejected'] or profile['trace_truncated']): raise ValueError('False profile parity')
            if keys and not profile['parity']: raise ValueError('Unestablished profile trace')
            if profile['bound'] and (profile['status']!='observed' or not profile['parity'] or profile['reference_present'] or profile['reason']!='synthetic_profile_bound'): raise ValueError('False profile binding')
            if profile['bound'] and (not f['source_preserved'] or not r['capture_control'] or not q['live_parity'] or not q['live_reference'].get('shader_source_preserved') or not q['live_reference'].get('live_background') or q['live_reference'].get('live_render_type')!=1): raise ValueError('Unestablished profile pass control')
            if profile['status']=='observed' and not profile['bound']: raise ValueError('Unestablished profile observation')
        if data['revision'] in ('shader_resources_v11','shader_resources_v12','shader_resources_v13','shader_resources_v14'):
            a=r['admission_probe']; exact(a,'status getter_names trace_truncated constructor_nil_parity')
            if a['status'] not in ('unknown','observed'): raise ValueError('Invalid admission status')
            boolean(a['trace_truncated']); boolean(a['constructor_nil_parity'])
            allowed_getters={'liveRenderType','homeKeyLiveRenderType','liveRenderedBackground','liveRenderedShaderSet','liveRenderedImageSet','liveRenderedArmedStateImageSet','displayProfile','passType'}
            if type(a['getter_names']) is not list or len(a['getter_names'])>16 or any(n not in allowed_getters for n in a['getter_names']): raise ValueError('Unsafe admission trace')
            candidate_view=data['revision'] in ('shader_resources_v12','shader_resources_v13','shader_resources_v14') and r['profile_probe']['bound']
            if a['status']=='observed' and (a['trace_truncated'] or not a['constructor_nil_parity'] or (r['view_created'] and not candidate_view)): raise ValueError('Unestablished admission trace')
            if a['status']=='unknown' and a['getter_names']: raise ValueError('Unknown admission trace')
        if r['status'] not in ('observed','unknown'): raise ValueError('Invalid renderer status')
        reasons={'pass_control_unavailable','renderer_class_unavailable','renderer_signature_unavailable','capture_control_failed','renderer_fixture_unavailable','renderer_view_unavailable','synthetic_renderer_three_phases','synthetic_renderer_no_three_phases','renderer_capture_exception','renderer_exception'}
        if r['reason'] not in reasons: raise ValueError('Invalid renderer reason')
        for k in ('view_created','shader_source_exact','capture_control','three_phases_observed'): boolean(r[k])
        kinds=r['view_kinds']
        allowed={'SCNView','MTKView','PKTexturedCardView','PKPayLaterCardView','UIImageView'}
        if type(kinds) is not list or len(kinds)>5 or any(k not in allowed for k in kinds): raise ValueError('Unexpected view kinds')
        samples=r['samples_rgb']
        if type(samples) is not list or len(samples)>6: raise ValueError('Unbounded samples')
        for sample in samples:
            if type(sample) is not list or len(sample) not in (0,3) or any(type(v) is not int or not 0<=v<=255 for v in sample): raise ValueError('Invalid RGB sample')
        if r['status']=='observed' and (not r['view_created'] or not r['shader_source_exact'] or not r['capture_control'] or len(samples)!=6): raise ValueError('Unestablished renderer observation')
        if r['status']=='observed' and (not q['live_parity'] or not q['live_reference'].get('shader_source_preserved') or not q['live_reference'].get('live_background') or q['live_reference'].get('live_render_type')!=1): raise ValueError('Unestablished renderer pass control')
        if r['three_phases_observed']:
            phases=[any(len(s)==3 and s[i]>200 and all(s[j]<=50 for j in range(3) if i!=j) for s in samples) for i in range(3)]
            if r['status']!='observed' or not all(phases) or r['reason']!='synthetic_renderer_three_phases': raise ValueError('False temporal color claim')
