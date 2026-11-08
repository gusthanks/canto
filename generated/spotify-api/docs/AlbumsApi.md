# \AlbumsApi

All URIs are relative to *https://api.spotify.com/v1*

Method | HTTP request | Description
------------- | ------------- | -------------
[**check_users_saved_albums**](AlbumsApi.md#check_users_saved_albums) | **GET** /me/albums/contains | Check User's Saved Albums 
[**get_an_album**](AlbumsApi.md#get_an_album) | **GET** /albums/{id} | Get Album 
[**get_an_albums_tracks**](AlbumsApi.md#get_an_albums_tracks) | **GET** /albums/{id}/tracks | Get Album Tracks 
[**get_an_artists_albums**](AlbumsApi.md#get_an_artists_albums) | **GET** /artists/{id}/albums | Get Artist's Albums 
[**get_multiple_albums**](AlbumsApi.md#get_multiple_albums) | **GET** /albums | Get Several Albums 
[**get_new_releases**](AlbumsApi.md#get_new_releases) | **GET** /browse/new-releases | Get New Releases 
[**get_users_saved_albums**](AlbumsApi.md#get_users_saved_albums) | **GET** /me/albums | Get User's Saved Albums 
[**remove_albums_user**](AlbumsApi.md#remove_albums_user) | **DELETE** /me/albums | Remove Users' Saved Albums 
[**save_albums_user**](AlbumsApi.md#save_albums_user) | **PUT** /me/albums | Save Albums for Current User 



## check_users_saved_albums

> Vec<bool> check_users_saved_albums(ids)
Check User's Saved Albums 

Check if one or more albums is already saved in the current Spotify user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_an_album

> models::AlbumObject get_an_album(id, market)
Get Album 

Get Spotify catalog information for a single album. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

[**models::AlbumObject**](AlbumObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_an_albums_tracks

> models::PagingSimplifiedTrackObject get_an_albums_tracks(id, market, limit, offset)
Get Album Tracks 

Get Spotify catalog information about an album’s tracks. Optional parameters can be used to limit the number of tracks returned. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSimplifiedTrackObject**](PagingSimplifiedTrackObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_an_artists_albums

> models::PagingArtistDiscographyAlbumObject get_an_artists_albums(id, include_groups, market, limit, offset)
Get Artist's Albums 

Get Spotify catalog information about an artist's albums. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** |  | [required] |
**include_groups** | Option<**String**> |  |  |
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 5]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingArtistDiscographyAlbumObject**](PagingArtistDiscographyAlbumObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_multiple_albums

> models::GetMultipleAlbums200Response get_multiple_albums(ids, market)
Get Several Albums 

Get Spotify catalog information for multiple albums identified by their Spotify IDs. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

[**models::GetMultipleAlbums200Response**](get_multiple_albums_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_new_releases

> models::GetNewReleases200Response get_new_releases(limit, offset)
Get New Releases 

Get a list of new album releases featured in Spotify (shown, for example, on a Spotify player’s “Browse” tab). 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::GetNewReleases200Response**](get_new_releases_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_saved_albums

> models::PagingSavedAlbumObject get_users_saved_albums(limit, offset, market)
Get User's Saved Albums 

Get a list of the albums saved in the current Spotify user's 'Your Music' library. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]
**market** | Option<**String**> |  |  |

### Return type

[**models::PagingSavedAlbumObject**](PagingSavedAlbumObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_albums_user

> remove_albums_user(ids, save_albums_user_request)
Remove Users' Saved Albums 

Remove one or more albums from the current user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**save_albums_user_request** | Option<[**SaveAlbumsUserRequest**](SaveAlbumsUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## save_albums_user

> save_albums_user(ids, save_albums_user_request)
Save Albums for Current User 

Save one or more albums to the current user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**save_albums_user_request** | Option<[**SaveAlbumsUserRequest**](SaveAlbumsUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

